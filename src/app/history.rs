use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

/// Maximum number of history entries kept on disk / in memory.
const MAX_ENTRIES: usize = 1000;

/// A single history entry.
#[derive(Debug, Clone)]
pub struct HistoryEntry {
    /// Unix timestamp (seconds) when the page was visited.
    pub timestamp: u64,
    /// The final resolved URL.
    pub url: String,
    /// Page `<title>` at the time of the visit (may be empty for failed loads).
    pub title: String,
}

/// Global browsing history store.  Loaded from disk at startup and updated
/// on every navigation.  Persists across sessions.
pub struct HistoryStore {
    pub entries: Vec<HistoryEntry>,
    path: PathBuf,
}

impl HistoryStore {
    /// Load (or create) the history file at `~/.forkit_history`.
    pub fn load() -> Self {
        let path = Self::default_path();
        let entries = Self::read_file(&path);
        HistoryStore { entries, path }
    }

    /// Record a visit.  Deduplicates consecutive identical URLs.
    /// Trims the list to `MAX_ENTRIES`.
    pub fn push(&mut self, url: &str, title: &str) {
        // Skip internal / blank pages
        if url == "about:blank" || url == "about:newtab" {
            return;
        }

        let ts = now_secs();

        // Don't push if it's identical to the most recent entry
        if let Some(last) = self.entries.last() {
            if last.url == url {
                return;
            }
        }

        self.entries.push(HistoryEntry {
            timestamp: ts,
            url: url.to_owned(),
            title: title.to_owned(),
        });

        // Trim oldest entries if we exceed the cap
        if self.entries.len() > MAX_ENTRIES {
            let overflow = self.entries.len() - MAX_ENTRIES;
            self.entries.drain(0..overflow);
        }

        // Append the new entry to disk without rewriting the whole file
        self.append_entry(self.entries.last().unwrap());
    }

    /// Return entries in reverse-chronological order (most recent first).
    pub fn recent(&self, limit: usize) -> Vec<&HistoryEntry> {
        self.entries.iter().rev().take(limit).collect()
    }

    /// Clear all history, both in memory and on disk.
    pub fn clear(&mut self) {
        self.entries.clear();
        let _ = fs::remove_file(&self.path);
    }

    // ---- private ----

    fn default_path() -> PathBuf {
        dirs_or_home().join(".forkit_history")
    }

    fn read_file(path: &PathBuf) -> Vec<HistoryEntry> {
        let file = match fs::File::open(path) {
            Ok(f)  => f,
            Err(_) => return Vec::new(),
        };
        let reader = BufReader::new(file);
        let mut entries = Vec::new();
        for line in reader.lines().flatten() {
            if let Some(entry) = parse_line(&line) {
                entries.push(entry);
            }
        }
        // If the file grew beyond the cap (e.g. due to a previous bug), trim in memory.
        if entries.len() > MAX_ENTRIES {
            let overflow = entries.len() - MAX_ENTRIES;
            entries.drain(0..overflow);
        }
        entries
    }

    fn append_entry(&self, entry: &HistoryEntry) {
        let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
        else {
            return;
        };
        // Format: "TIMESTAMP\tURL\tTITLE\n"
        let title_escaped = entry.title.replace('\t', " ").replace('\n', " ");
        let url_escaped   = entry.url  .replace('\t', "%09").replace('\n', "%0A");
        let _ = writeln!(file, "{}\t{}\t{}", entry.timestamp, url_escaped, title_escaped);
    }
}

fn parse_line(line: &str) -> Option<HistoryEntry> {
    let mut parts = line.splitn(3, '\t');
    let ts_str = parts.next()?;
    let url    = parts.next()?;
    let title  = parts.next().unwrap_or("").to_owned();
    let timestamp = ts_str.parse::<u64>().ok()?;
    if url.is_empty() { return None; }
    Some(HistoryEntry {
        timestamp,
        url: url.to_owned(),
        title,
    })
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Returns the user's home directory.
fn dirs_or_home() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}
