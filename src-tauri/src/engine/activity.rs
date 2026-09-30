use std::collections::VecDeque;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

const CAPACITY: usize = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ActivityLevel {
    Info,
    Warn,
    Error,
}

/// User-visible record of what the engine did, shown in the diagnostics view.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityEntry {
    pub timestamp_ms: u64,
    pub level: ActivityLevel,
    pub window: Option<String>,
    pub message: String,
}

#[derive(Default)]
pub struct ActivityLog {
    entries: VecDeque<ActivityEntry>,
}

impl ActivityLog {
    pub fn push(
        &mut self,
        level: ActivityLevel,
        window: Option<String>,
        message: String,
    ) -> ActivityEntry {
        match level {
            ActivityLevel::Info => tracing::info!(window = window.as_deref(), "{message}"),
            ActivityLevel::Warn => tracing::warn!(window = window.as_deref(), "{message}"),
            ActivityLevel::Error => tracing::error!(window = window.as_deref(), "{message}"),
        }
        let entry = ActivityEntry {
            timestamp_ms: now_ms(),
            level,
            window,
            message,
        };
        if self.entries.len() == CAPACITY {
            self.entries.pop_front();
        }
        self.entries.push_back(entry.clone());
        entry
    }

    pub fn entries(&self) -> Vec<ActivityEntry> {
        self.entries.iter().cloned().collect()
    }
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}
