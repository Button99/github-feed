// Run the real CLI against a fake gh executable: no login or network required.
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

struct FakeGithub(PathBuf);

impl FakeGithub {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "github-feed-test-{}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&dir).unwrap();
        let executable = dir.join("gh");
        fs::write(
            &executable,
            r#"#!/bin/sh
case "$4" in
    user) printf '%s\n' "${FAKE_LOGIN:-test-user}" ;;
    users/*/received_events?per_page=100)
        printf 'fetch\n' >> "$XDG_CACHE_HOME/fetches"
        if [ "$FAKE_FAIL" = 1 ]; then printf 'Network unavailable\n' >&2; exit 1; fi
        i=1
        while [ "$i" -le 25 ]; do
            if [ "$i" -le 5 ]; then repo=other/repo; else repo=owner/repo; fi
            printf '2026-10-24T12:00:00Z\tactor%s\tWatchEvent\t%s\n' "$i" "$repo"
            i=$((i + 1))
        done
        ;;
    *) printf 'Unexpected endpoint\n' >&2; exit 1 ;;
esac
"#,
        )
        .unwrap();
        fs::set_permissions(executable, fs::Permissions::from_mode(0o700)).unwrap();
        Self(dir)
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_github-feed"))
            .args(args)
            .env("PATH", &self.0)
            .env("XDG_CACHE_HOME", &self.0)
            .output()
            .unwrap()
    }
}

impl Drop for FakeGithub {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn event_lines(output: &Output) -> Vec<String> {
    assert!(output.status.success(), "{:?}", output);
    String::from_utf8(output.stdout.clone())
        .unwrap()
        .lines()
        .filter(|line| line.starts_with("2026-"))
        .map(str::to_owned)
        .collect()
}

#[test]
fn defaults_to_first_ten_events_in_feed_order() {
    let output = FakeGithub::new().run(&["check"]);
    let lines = event_lines(&output);
    assert_eq!(lines.len(), 10);
    assert!(lines[0].contains("\tactor1\t"));
    assert!(lines[9].contains("\tactor10\t"));
}

#[test]
fn accepts_long_and_short_limits() {
    let gh = FakeGithub::new();
    assert_eq!(event_lines(&gh.run(&["check", "--limit", "20"])).len(), 20);
    assert_eq!(event_lines(&gh.run(&["check", "-l", "5"])).len(), 5);
}

#[test]
fn filters_before_applying_limit() {
    let output = FakeGithub::new().run(&["check", "--repo", "OWNER/REPO", "-l", "10"]);
    let lines = event_lines(&output);
    assert_eq!(lines.len(), 10);
    assert!(lines.iter().all(|line| line.ends_with("\towner/repo")));
    assert!(lines[0].contains("\tactor6\t"));
    assert!(lines[9].contains("\tactor15\t"));
}

#[test]
fn handles_zero_and_limits_larger_than_available_activity() {
    let gh = FakeGithub::new();
    assert!(event_lines(&gh.run(&["check", "-l", "0"])).is_empty());
    assert_eq!(event_lines(&gh.run(&["check", "-l", "100"])).len(), 25);
}

#[test]
fn reports_no_matching_repository() {
    let output = FakeGithub::new().run(&["check", "--repo", "missing/repo"]);
    assert!(output.status.success());
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("No matching activity")
    );
}

#[test]
fn rejects_invalid_or_missing_limit_values() {
    let gh = FakeGithub::new();
    for args in [
        vec!["check", "--limit", "abc"],
        vec!["check", "--limit", "-1"],
        vec!["check", "--limit"],
    ] {
        let output = gh.run(&args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(String::from_utf8(output.stderr).unwrap().contains("error:"));
    }
}

#[test]
fn fresh_cache_is_shared_across_limits_and_filters_and_refresh_bypasses_it() {
    let gh = FakeGithub::new();
    assert_eq!(event_lines(&gh.run(&["check", "-l", "5"])).len(), 5);
    let cached = gh.run(&["check", "--repo", "owner/repo", "-l", "20"]);
    assert_eq!(event_lines(&cached).len(), 20);
    assert!(
        String::from_utf8(cached.stderr)
            .unwrap()
            .contains("Feed cache: cache;")
    );
    assert_eq!(
        fs::read_to_string(gh.0.join("fetches"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    event_lines(&gh.run(&["check", "--refresh"]));
    assert_eq!(
        fs::read_to_string(gh.0.join("fetches"))
            .unwrap()
            .lines()
            .count(),
        2
    );
}

#[test]
fn failed_refresh_preserves_cache_and_reports_stale_results() {
    let gh = FakeGithub::new();
    let first = event_lines(&gh.run(&["check"]));
    let path = gh.0.join("github-feed/github.com-test-user.tsv");
    let original = fs::read(&path).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_github-feed"))
        .args(["check", "--refresh"])
        .env("PATH", &gh.0)
        .env("XDG_CACHE_HOME", &gh.0)
        .env("FAKE_FAIL", "1")
        .output()
        .unwrap();
    assert_eq!(event_lines(&output), first);
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("Feed cache: stale;")
    );
    assert_eq!(fs::read(path).unwrap(), original);
}

#[test]
fn expired_and_corrupt_caches_are_refetched() {
    let gh = FakeGithub::new();
    gh.run(&["check"]);
    let path = gh.0.join("github-feed/github.com-test-user.tsv");
    let text = fs::read_to_string(&path).unwrap();
    let (_, activity) = text.split_once('\n').unwrap();
    fs::write(&path, format!("github-feed-v1\t1\n{activity}")).unwrap();
    assert_eq!(event_lines(&gh.run(&["check"])).len(), 10);
    fs::write(&path, "broken cache").unwrap();
    assert_eq!(event_lines(&gh.run(&["check"])).len(), 10);
    assert_eq!(
        fs::read_to_string(gh.0.join("fetches"))
            .unwrap()
            .lines()
            .count(),
        3
    );
}

#[test]
fn account_switch_does_not_reuse_another_accounts_cache() {
    let gh = FakeGithub::new();
    gh.run(&["check"]);
    let output = Command::new(env!("CARGO_BIN_EXE_github-feed"))
        .args(["check"])
        .env("PATH", &gh.0)
        .env("XDG_CACHE_HOME", &gh.0)
        .env("FAKE_LOGIN", "other-user")
        .env("FAKE_FAIL", "1")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
}

#[test]
fn cache_files_are_private() {
    let gh = FakeGithub::new();
    gh.run(&["check"]);
    let dir = gh.0.join("github-feed");
    assert_eq!(
        fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(dir.join("github.com-test-user.tsv"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
}
