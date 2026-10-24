use crate::cache;
use std::process::Command;

/// Run gh directly so credentials stay managed by GitHub CLI.
fn api(args: &[&str]) -> Result<String, String> {
    let output = Command::new("gh")
        .args(["api", "--hostname", "github.com"])
        .args(args)
        .output()
        .map_err(|error| {
            format!("Could not run GitHub CLI: {error}. Install gh and run `gh auth login`.")
        })?;

    if !output.status.success() {
        // Keep gh's actionable API/authentication error; escape terminal controls.
        let detail = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "GitHub request failed: {}",
            safe_text(detail.trim())
        ));
    }

    String::from_utf8(output.stdout)
        .map_err(|error| format!("GitHub CLI returned invalid UTF-8: {error}"))
}

pub struct Activity {
    pub text: String,
    pub source: &'static str,
    pub saved_at: u64,
    pub warning: Option<String>,
}

pub fn received_activity(refresh: bool) -> Result<Activity, String> {
    // gh scopes its HTTP cache to the request, including authentication.
    // Resolve identity before using an account's saved activity.
    let username = api(&["user", "--cache", "24h", "--jq", ".login"])?;
    let username = username.trim();
    if username.is_empty()
        || !username
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err("GitHub returned an invalid account login.".into());
    }

    let cached = cache::load(username);
    if !refresh
        && let Some(snapshot) = &cached
        && cache::now().saturating_sub(snapshot.saved_at) < cache::TTL
    {
        return Ok(Activity {
            text: snapshot.activity.clone(),
            source: "cache",
            saved_at: snapshot.saved_at,
            warning: None,
        });
    }
    let endpoint = format!("users/{username}/received_events?per_page=100");
    // @tsv escapes control characters in remote fields. Each page is formatted
    // by gh separately; no JSON dependency is needed for this first milestone.
    let fetched = api(&[
        &endpoint,
        "--paginate",
        "--jq",
        ".[] | [.created_at, .actor.login, .type, .repo.name] | @tsv",
    ]);
    match fetched {
        Ok(text) => {
            let saved_at = cache::now();
            let warning = cache::save(username, &text, saved_at)
                .err()
                .map(|error| format!("Could not save activity cache: {error}"));
            Ok(Activity {
                text,
                source: "live",
                saved_at,
                warning,
            })
        }
        Err(error) => match cached {
            Some(snapshot) => Ok(Activity {
                text: snapshot.activity,
                source: "stale",
                saved_at: snapshot.saved_at,
                warning: Some(format!("{error}. Showing saved activity.")),
            }),
            None => Err(error),
        },
    }
}

pub fn activity_lines<'a>(activity: &'a str, repo: Option<&str>) -> Vec<&'a str> {
    activity
        .lines()
        .filter(|line| !line.is_empty())
        .filter(|line| match repo {
            Some(repo) => line
                .split('\t')
                .nth(3)
                .is_some_and(|name| name.eq_ignore_ascii_case(repo)),
            None => true,
        })
        .collect()
}

pub fn safe_text(text: &str) -> String {
    text.chars().flat_map(char::escape_default).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_exact_repo_without_matching_actor_or_similar_names() {
        let activity = "2026-10-24T12:00:00Z\talice\tWatchEvent\tOwner/Repo\n\
                        2026-10-24T12:01:00Z\tbob\tFutureEvent\towner/repo-other\n";
        assert_eq!(activity_lines(activity, Some("owner/repo")).len(), 1);
        assert_eq!(activity_lines(activity, Some("alice")).len(), 0);
        assert_eq!(activity_lines(activity, None).len(), 2);
        assert!(activity_lines("", None).is_empty());
    }

    #[test]
    fn escapes_remote_terminal_controls() {
        assert_eq!(safe_text("hello\x1b[31m\n"), "hello\\u{1b}[31m\\n");
    }
}
