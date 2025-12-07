//! Recent workspaces tracking
//!
//! Stores recently opened workspaces in ~/.config/fackr/recents.json

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recent {
    pub path: PathBuf,
    pub label: String,
    pub last_opened: u64, // Unix timestamp
    pub open_count: u32,
}

impl Recent {
    pub fn new(path: PathBuf) -> Self {
        let label = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string());

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        Self {
            path,
            label,
            last_opened: timestamp,
            open_count: 1,
        }
    }
}

/// Get the path to the recents file
fn recents_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("fackr")
        .join("recents.json")
}

/// Load recent workspaces from disk
pub fn recents_load() -> Vec<Recent> {
    let path = recents_path();
    if !path.exists() {
        return Vec::new();
    }

    match fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

/// Save recent workspaces to disk
pub fn recents_save(recents: &[Recent]) -> Result<()> {
    let path = recents_path();

    // Ensure config directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let content = serde_json::to_string_pretty(recents)?;
    fs::write(&path, content)?;
    Ok(())
}

/// Add or update a workspace in recents
pub fn recents_add_or_update(path: &Path) -> Result<()> {
    let mut recents = recents_load();
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    // Find existing entry
    if let Some(existing) = recents.iter_mut().find(|r| r.path == canonical) {
        existing.last_opened = timestamp;
        existing.open_count += 1;
    } else {
        recents.push(Recent::new(canonical));
    }

    // Sort by last_opened descending (most recent first)
    recents.sort_by(|a, b| b.last_opened.cmp(&a.last_opened));

    // Keep only the most recent 50 entries
    recents.truncate(50);

    recents_save(&recents)
}

/// Get recent workspaces, sorted by most recently opened
pub fn recents_get() -> Vec<Recent> {
    let mut recents = recents_load();
    // Filter out non-existent directories
    recents.retain(|r| r.path.exists());
    recents
}
