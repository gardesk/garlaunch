use super::{Action, Item, Mode};
use anyhow::Result;
use std::collections::HashSet;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

/// Run arbitrary commands from PATH
pub struct RunMode {
    items: Vec<Item>,
}

impl RunMode {
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    /// Get directories from PATH
    fn path_dirs() -> Vec<PathBuf> {
        std::env::var("PATH")
            .unwrap_or_default()
            .split(':')
            .map(PathBuf::from)
            .filter(|p| p.exists())
            .collect()
    }

    /// Check if a file is executable
    fn is_executable(path: &PathBuf) -> bool {
        if let Ok(metadata) = fs::metadata(path) {
            let permissions = metadata.permissions();
            permissions.mode() & 0o111 != 0
        } else {
            false
        }
    }
}

impl Mode for RunMode {
    fn name(&self) -> &str {
        "run"
    }

    fn load(&mut self) -> Result<()> {
        let mut seen = HashSet::new();

        for dir in Self::path_dirs() {
            if let Ok(entries) = fs::read_dir(&dir) {
                for entry in entries.filter_map(|e| e.ok()) {
                    let path = entry.path();

                    // Skip directories
                    if path.is_dir() {
                        continue;
                    }

                    // Skip non-executable files
                    if !Self::is_executable(&path) {
                        continue;
                    }

                    if let Some(name) = path.file_name() {
                        let name_str = name.to_string_lossy().to_string();

                        // Skip duplicates (first in PATH wins)
                        if seen.contains(&name_str) {
                            continue;
                        }
                        seen.insert(name_str.clone());

                        // Skip hidden files
                        if name_str.starts_with('.') {
                            continue;
                        }

                        let item = Item::new(&name_str, &name_str).with_data(serde_json::json!({
                            "path": path.to_string_lossy(),
                        }));

                        self.items.push(item);
                    }
                }
            }
        }

        // Sort by name
        self.items.sort_by(|a, b| a.name.cmp(&b.name));

        tracing::info!("Loaded {} executables from PATH", self.items.len());

        Ok(())
    }

    fn items(&self) -> &[Item] {
        &self.items
    }

    fn activate(&self, item: &Item) -> Result<Action> {
        Ok(Action::Launch(item.name.clone()))
    }
}
