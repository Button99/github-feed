use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub const TTL: u64 = 300;
static NEXT_FILE: AtomicUsize = AtomicUsize::new(0);

pub struct Snapshot {
    pub saved_at: u64,
    pub activity: String,
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn directory() -> io::Result<PathBuf> {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .ok_or_else(|| io::Error::other("Neither XDG_CACHE_HOME nor HOME is set"))?;
    Ok(base.join("github-feed"))
}

pub fn load(account: &str) -> Option<Snapshot> {
    let path = directory().ok()?.join(format!("github.com-{account}.tsv"));
    if fs::metadata(&path).ok()?.len() > 5_000_000 {
        return None;
    }
    let text = fs::read_to_string(path).ok()?;
    let (header, activity) = text.split_once('\n')?;
    let saved_at: u64 = header.strip_prefix("github-feed-v1\t")?.parse().ok()?;
    if saved_at > now() {
        return None;
    }
    if activity
        .lines()
        .any(|line| line.split('\t').count() != 4 || line.contains('\x1b'))
    {
        return None;
    }
    Some(Snapshot {
        saved_at,
        activity: activity.to_owned(),
    })
}

pub fn save(account: &str, activity: &str, saved_at: u64) -> io::Result<()> {
    let dir = directory()?;
    fs::create_dir_all(&dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;
    }
    let path = dir.join(format!("github.com-{account}.tsv"));
    let temporary = dir.join(format!(
        ".{}-{}.tmp",
        std::process::id(),
        NEXT_FILE.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        writeln!(file, "github-feed-v1\t{saved_at}")?;
        file.write_all(activity.as_bytes())?;
        file.sync_all()?;
        fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}
