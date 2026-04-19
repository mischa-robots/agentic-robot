// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Michael Schaefer <https://github.com/mischa-robots/agentic-robot>

//! On-disk history storage for frames, reasoning, and commands.
//!
//! Each autonomous cycle is stored as a timestamped directory containing
//! the captured frame and a JSON metadata file.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::config;

/// A single history entry representing one autonomous cycle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub frame_path: Option<String>,
    pub reasoning: Vec<String>,
    pub command: Option<CommandRecord>,
}

/// A recorded motor command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandRecord {
    pub left: f32,
    pub right: f32,
    pub timestamp: DateTime<Utc>,
}

/// Trait for history storage implementations.
pub trait HistoryStore: Send + Sync {
    /// Create a new history entry with a captured frame.
    fn create_entry(&mut self, frame_path: &str) -> Result<String, std::io::Error>;

    /// Append a reasoning message to the current (latest) entry.
    fn append_reasoning(&mut self, message: &str) -> Result<(), std::io::Error>;

    /// Record a motor command in the current entry.
    fn record_command(&mut self, left: f32, right: f32) -> Result<(), std::io::Error>;

    /// Get the N most recent history entries.
    fn recent(&self, count: usize) -> Vec<HistoryEntry>;

    /// Get total number of entries.
    fn entry_count(&self) -> u64;
}

/// Disk-based history storage.
pub struct DiskHistoryStore {
    base_dir: PathBuf,
    current_entry: Option<HistoryEntry>,
    max_entries: usize,
}

impl DiskHistoryStore {
    pub fn new(max_entries: usize) -> Self {
        let base_dir = config::data_dir().join("history");
        Self {
            base_dir,
            current_entry: None,
            max_entries,
        }
    }

    pub fn with_base_dir(base_dir: PathBuf, max_entries: usize) -> Self {
        Self {
            base_dir,
            current_entry: None,
            max_entries,
        }
    }

    fn entry_dir(&self, id: &str) -> PathBuf {
        self.base_dir.join(id)
    }

    fn save_entry(&self, entry: &HistoryEntry) -> Result<(), std::io::Error> {
        let dir = self.entry_dir(&entry.id);
        std::fs::create_dir_all(&dir)?;
        let meta_path = dir.join("entry.json");
        let json = serde_json::to_string_pretty(entry)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(meta_path, json)?;
        Ok(())
    }

    fn load_entry(dir: &Path) -> Option<HistoryEntry> {
        let meta_path = dir.join("entry.json");
        let content = std::fs::read_to_string(meta_path).ok()?;
        serde_json::from_str(&content).ok()
    }

    /// Enforce retention by removing oldest entries.
    fn enforce_retention(&self) {
        let entries = self.list_entry_dirs();
        if entries.len() > self.max_entries {
            let to_remove = entries.len() - self.max_entries;
            for dir in entries.into_iter().take(to_remove) {
                if let Err(e) = std::fs::remove_dir_all(&dir) {
                    warn!(?dir, %e, "failed to remove old history entry");
                }
            }
        }
    }

    fn list_entry_dirs(&self) -> Vec<PathBuf> {
        let Ok(entries) = std::fs::read_dir(&self.base_dir) else {
            return Vec::new();
        };

        let mut dirs: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();

        dirs.sort();
        dirs
    }
}

impl HistoryStore for DiskHistoryStore {
    fn create_entry(&mut self, frame_path: &str) -> Result<String, std::io::Error> {
        let now = Utc::now();
        let id = now.format("%Y-%m-%dT%H-%M-%S").to_string();

        let entry = HistoryEntry {
            id: id.clone(),
            timestamp: now,
            frame_path: Some(frame_path.to_string()),
            reasoning: Vec::new(),
            command: None,
        };

        self.save_entry(&entry)?;
        self.current_entry = Some(entry);
        self.enforce_retention();

        Ok(id)
    }

    fn append_reasoning(&mut self, message: &str) -> Result<(), std::io::Error> {
        if let Some(entry) = &mut self.current_entry {
            entry.reasoning.push(message.to_string());
            let entry_clone = entry.clone();
            self.save_entry(&entry_clone)?;
        }
        Ok(())
    }

    fn record_command(&mut self, left: f32, right: f32) -> Result<(), std::io::Error> {
        if let Some(entry) = &mut self.current_entry {
            entry.command = Some(CommandRecord {
                left,
                right,
                timestamp: Utc::now(),
            });
            let entry_clone = entry.clone();
            self.save_entry(&entry_clone)?;
        }
        Ok(())
    }

    fn recent(&self, count: usize) -> Vec<HistoryEntry> {
        let dirs = self.list_entry_dirs();
        dirs.iter()
            .rev()
            .take(count)
            .filter_map(|d| Self::load_entry(d))
            .collect()
    }

    fn entry_count(&self) -> u64 {
        self.list_entry_dirs().len() as u64
    }
}

/// In-memory history store for testing.
#[cfg(test)]
pub struct InMemoryHistoryStore {
    entries: Vec<HistoryEntry>,
    max_entries: usize,
}

#[cfg(test)]
impl InMemoryHistoryStore {
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: Vec::new(),
            max_entries,
        }
    }
}

#[cfg(test)]
impl HistoryStore for InMemoryHistoryStore {
    fn create_entry(&mut self, frame_path: &str) -> Result<String, std::io::Error> {
        let now = Utc::now();
        let id = format!("entry-{}", self.entries.len());

        let entry = HistoryEntry {
            id: id.clone(),
            timestamp: now,
            frame_path: Some(frame_path.to_string()),
            reasoning: Vec::new(),
            command: None,
        };

        self.entries.push(entry);

        // Enforce retention
        while self.entries.len() > self.max_entries {
            self.entries.remove(0);
        }

        Ok(id)
    }

    fn append_reasoning(&mut self, message: &str) -> Result<(), std::io::Error> {
        if let Some(entry) = self.entries.last_mut() {
            entry.reasoning.push(message.to_string());
        }
        Ok(())
    }

    fn record_command(&mut self, left: f32, right: f32) -> Result<(), std::io::Error> {
        if let Some(entry) = self.entries.last_mut() {
            entry.command = Some(CommandRecord {
                left,
                right,
                timestamp: Utc::now(),
            });
        }
        Ok(())
    }

    fn recent(&self, count: usize) -> Vec<HistoryEntry> {
        self.entries.iter().rev().take(count).cloned().collect()
    }

    fn entry_count(&self) -> u64 {
        self.entries.len() as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_memory_store_creates_entries() {
        let mut store = InMemoryHistoryStore::new(100);
        let id = store.create_entry("/tmp/frame.jpg").unwrap();
        assert_eq!(id, "entry-0");
        assert_eq!(store.entry_count(), 1);
    }

    #[test]
    fn in_memory_store_appends_reasoning() {
        let mut store = InMemoryHistoryStore::new(100);
        store.create_entry("/tmp/frame.jpg").unwrap();
        store.append_reasoning("I see a wall ahead").unwrap();
        store.append_reasoning("Turning right").unwrap();

        let entries = store.recent(1);
        assert_eq!(entries[0].reasoning.len(), 2);
        assert_eq!(entries[0].reasoning[0], "I see a wall ahead");
    }

    #[test]
    fn in_memory_store_records_command() {
        let mut store = InMemoryHistoryStore::new(100);
        store.create_entry("/tmp/frame.jpg").unwrap();
        store.record_command(0.5, -0.3).unwrap();

        let entries = store.recent(1);
        let cmd = entries[0].command.as_ref().unwrap();
        assert!((cmd.left - 0.5).abs() < f32::EPSILON);
        assert!((cmd.right - (-0.3)).abs() < f32::EPSILON);
    }

    #[test]
    fn in_memory_store_enforces_retention() {
        let mut store = InMemoryHistoryStore::new(3);
        for i in 0..5 {
            store.create_entry(&format!("/tmp/frame{i}.jpg")).unwrap();
        }
        assert_eq!(store.entry_count(), 3);
    }

    #[test]
    fn in_memory_store_recent_returns_newest_first() {
        let mut store = InMemoryHistoryStore::new(100);
        store.create_entry("/tmp/frame0.jpg").unwrap();
        store.create_entry("/tmp/frame1.jpg").unwrap();
        store.create_entry("/tmp/frame2.jpg").unwrap();

        let entries = store.recent(2);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].id, "entry-2");
        assert_eq!(entries[1].id, "entry-1");
    }

    #[test]
    fn disk_store_creates_and_loads() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = DiskHistoryStore::with_base_dir(dir.path().to_path_buf(), 100);

        let id = store.create_entry("/tmp/frame.jpg").unwrap();
        assert!(!id.is_empty());

        let entries = store.recent(1);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].frame_path.as_deref(), Some("/tmp/frame.jpg"));
    }

    #[test]
    fn disk_store_enforces_retention() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = DiskHistoryStore::with_base_dir(dir.path().to_path_buf(), 2);

        // Create entries with unique timestamps
        for i in 0..4 {
            let now = Utc::now();
            let id = format!("{}-{i:04}", now.format("%Y-%m-%dT%H-%M-%S"));
            let entry = HistoryEntry {
                id: id.clone(),
                timestamp: now,
                frame_path: Some(format!("/tmp/frame{i}.jpg")),
                reasoning: Vec::new(),
                command: None,
            };
            store.save_entry(&entry).unwrap();
            store.current_entry = Some(entry);
            store.enforce_retention();
        }

        assert!(store.entry_count() <= 2);
    }
}
