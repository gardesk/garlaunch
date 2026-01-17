use anyhow::Result;
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Default half-life in hours (1 week)
const DEFAULT_HALF_LIFE_HOURS: f64 = 168.0;

/// Entry in the frecency store
#[derive(Debug, Clone)]
pub struct FrecencyEntry {
    pub score: f64,
    pub last_access: u64,
}

/// Frecency store for a mode
pub struct FrecencyStore {
    entries: HashMap<String, FrecencyEntry>,
    half_life_hours: f64,
    path: PathBuf,
}

impl FrecencyStore {
    /// Create a new store for the given mode
    pub fn new(mode_name: &str) -> Self {
        let cache_dir = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("~/.cache"))
            .join("garlaunch");

        Self {
            entries: HashMap::new(),
            half_life_hours: DEFAULT_HALF_LIFE_HOURS,
            path: cache_dir.join(format!("{}.cache", mode_name)),
        }
    }

    /// Create in-memory store for testing
    #[cfg(test)]
    pub fn new_in_memory(half_life_hours: f64) -> Self {
        Self {
            entries: HashMap::new(),
            half_life_hours,
            path: PathBuf::new(),
        }
    }

    /// Load from cache file
    pub fn load(mode_name: &str) -> Result<Self> {
        let mut store = Self::new(mode_name);

        if !store.path.exists() {
            return Ok(store);
        }

        let file = fs::File::open(&store.path)?;
        let reader = BufReader::new(file);

        for line in reader.lines() {
            let line = line?;
            let line = line.trim();

            // Skip comments and empty lines
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Parse: id\tscore\tlast_access
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 3 {
                let id = parts[0].to_string();
                let score: f64 = parts[1].parse().unwrap_or(0.0);
                let last_access: u64 = parts[2].parse().unwrap_or(0);

                store.entries.insert(id, FrecencyEntry { score, last_access });
            }
        }

        tracing::debug!(
            "Loaded {} frecency entries from {:?}",
            store.entries.len(),
            store.path
        );

        Ok(store)
    }

    /// Save to cache file
    pub fn save(&self) -> Result<()> {
        // Ensure cache directory exists
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut file = fs::File::create(&self.path)?;
        writeln!(file, "# garlaunch frecency cache v1")?;

        // Sort by score descending for readability
        let mut entries: Vec<_> = self.entries.iter().collect();
        entries.sort_by(|a, b| b.1.score.partial_cmp(&a.1.score).unwrap());

        for (id, entry) in entries {
            writeln!(file, "{}\t{:.6}\t{}", id, entry.score, entry.last_access)?;
        }

        tracing::debug!(
            "Saved {} frecency entries to {:?}",
            self.entries.len(),
            self.path
        );

        Ok(())
    }

    /// Record an item selection (bumps score)
    pub fn record(&mut self, item_id: &str) {
        let now = current_time_secs();

        let entry = self
            .entries
            .entry(item_id.to_string())
            .or_insert(FrecencyEntry {
                score: 0.0,
                last_access: now,
            });

        // Calculate decay since last access
        let elapsed_hours = (now.saturating_sub(entry.last_access)) as f64 / 3600.0;
        let decayed_score = entry.score * decay_factor(elapsed_hours, self.half_life_hours);

        // Bump score and update timestamp
        entry.score = decayed_score + 1.0;
        entry.last_access = now;

        tracing::trace!(
            "Recorded frecency for '{}': score={:.2}, elapsed_hours={:.1}",
            item_id,
            entry.score,
            elapsed_hours
        );
    }

    /// Get frecency score for an item (with current decay applied)
    pub fn score(&self, item_id: &str) -> f64 {
        let Some(entry) = self.entries.get(item_id) else {
            return 0.0;
        };

        let now = current_time_secs();
        let elapsed_hours = (now.saturating_sub(entry.last_access)) as f64 / 3600.0;
        entry.score * decay_factor(elapsed_hours, self.half_life_hours)
    }

    /// Get all entry IDs sorted by score (highest first)
    pub fn sorted_ids(&self) -> Vec<&str> {
        let mut entries: Vec<_> = self
            .entries
            .iter()
            .map(|(id, _)| (id.as_str(), self.score(id)))
            .collect();

        entries.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        entries.into_iter().map(|(id, _)| id).collect()
    }

    /// Check if store has any entries
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for FrecencyStore {
    fn default() -> Self {
        Self::new("default")
    }
}

/// Calculate decay factor based on elapsed time and half-life
fn decay_factor(elapsed_hours: f64, half_life_hours: f64) -> f64 {
    2.0_f64.powf(-elapsed_hours / half_life_hours)
}

/// Get current time as Unix timestamp in seconds
fn current_time_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_increases_score() {
        let mut store = FrecencyStore::new_in_memory(168.0);

        store.record("firefox");
        let score1 = store.score("firefox");
        assert!(score1 > 0.0);

        // Recording again should increase score
        store.record("firefox");
        let score2 = store.score("firefox");
        assert!(score2 > score1);
    }

    #[test]
    fn test_new_entry_score_is_one() {
        let mut store = FrecencyStore::new_in_memory(168.0);
        store.record("new_app");

        // New entry should have score ~1.0 (no decay yet)
        let score = store.score("new_app");
        assert!((score - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_sorted_ids() {
        let mut store = FrecencyStore::new_in_memory(168.0);

        // Record app_a twice, app_b once
        store.record("app_a");
        store.record("app_a");
        store.record("app_b");

        let sorted = store.sorted_ids();
        assert_eq!(sorted.len(), 2);
        assert_eq!(sorted[0], "app_a");
        assert_eq!(sorted[1], "app_b");
    }

    #[test]
    fn test_decay_factor() {
        // After one half-life, decay should be 0.5
        let factor = decay_factor(168.0, 168.0);
        assert!((factor - 0.5).abs() < 0.001);

        // After two half-lives, decay should be 0.25
        let factor = decay_factor(336.0, 168.0);
        assert!((factor - 0.25).abs() < 0.001);

        // No time elapsed, no decay
        let factor = decay_factor(0.0, 168.0);
        assert!((factor - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_unknown_item_score_is_zero() {
        let store = FrecencyStore::new_in_memory(168.0);
        assert_eq!(store.score("nonexistent"), 0.0);
    }
}
