use super::{Action, Item, Mode};
use anyhow::Result;
use freedesktop_entry_parser::Entry;
use std::path::PathBuf;
use walkdir::WalkDir;

/// Desktop application launcher mode
pub struct DrunMode {
    items: Vec<Item>,
}

impl DrunMode {
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    /// Get XDG data directories for .desktop files
    fn desktop_dirs() -> Vec<PathBuf> {
        let mut dirs = Vec::new();

        // User applications
        if let Some(data_home) = dirs::data_dir() {
            dirs.push(data_home.join("applications"));
        }

        // System applications
        dirs.push(PathBuf::from("/usr/share/applications"));
        dirs.push(PathBuf::from("/usr/local/share/applications"));

        // NixOS applications
        dirs.push(PathBuf::from("/run/current-system/sw/share/applications"));

        // Flatpak applications
        if let Some(data_home) = dirs::data_dir() {
            dirs.push(data_home.join("flatpak/exports/share/applications"));
        }
        dirs.push(PathBuf::from("/var/lib/flatpak/exports/share/applications"));

        // Snap applications
        dirs.push(PathBuf::from("/var/lib/snapd/desktop/applications"));

        dirs
    }

    /// Parse a .desktop file into an Item
    fn parse_desktop_file(path: &PathBuf) -> Option<Item> {
        // Use Entry::parse_file which handles reading and parsing
        let entry = match Entry::parse_file(path) {
            Ok(e) => e,
            Err(e) => {
                tracing::trace!("Failed to parse {:?}: {:?}", path, e);
                return None;
            }
        };

        let desktop_entry = entry.section("Desktop Entry");

        // Skip entries that shouldn't be shown
        if desktop_entry.attr("NoDisplay") == Some("true") {
            tracing::trace!("Skipping {:?}: NoDisplay=true", path);
            return None;
        }
        if desktop_entry.attr("Hidden") == Some("true") {
            tracing::trace!("Skipping {:?}: Hidden=true", path);
            return None;
        }

        let name = match desktop_entry.attr("Name") {
            Some(n) => n,
            None => {
                tracing::trace!("Skipping {:?}: no Name attribute", path);
                return None;
            }
        };

        let exec = match desktop_entry.attr("Exec") {
            Some(e) => e,
            None => {
                tracing::trace!("Skipping {:?}: no Exec attribute", path);
                return None;
            }
        };

        let comment = desktop_entry.attr("Comment").map(String::from);
        let icon = desktop_entry.attr("Icon").map(String::from);

        // Clean up the Exec string (remove %f, %F, %u, %U, etc.)
        let exec_clean = clean_exec(exec);

        let id = path.file_stem()?.to_string_lossy().to_string();

        let mut item = Item::new(&id, name).with_data(serde_json::json!({
            "exec": exec_clean,
            "path": path.to_string_lossy(),
        }));

        if let Some(comment) = comment {
            item = item.with_description(comment);
        }

        if let Some(icon) = icon {
            item = item.with_icon(icon);
        }

        Some(item)
    }
}

impl Mode for DrunMode {
    fn name(&self) -> &str {
        "drun"
    }

    fn load(&mut self) -> Result<()> {
        let mut seen = std::collections::HashSet::new();

        for dir in Self::desktop_dirs() {
            tracing::debug!("Checking directory: {:?} (exists: {})", dir, dir.exists());
            if !dir.exists() {
                continue;
            }

            let mut count_in_dir = 0;
            for entry in WalkDir::new(&dir)
                .max_depth(2)
                .follow_links(true)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                let path = entry.path();
                if path.extension().is_some_and(|ext| ext == "desktop") {
                    let path_buf = path.to_path_buf();

                    // Skip duplicates (first one wins)
                    let file_name = path.file_name().map(|s| s.to_string_lossy().to_string());
                    if let Some(name) = file_name {
                        if seen.contains(&name) {
                            continue;
                        }
                        seen.insert(name);
                    }

                    match Self::parse_desktop_file(&path_buf) {
                        Some(item) => {
                            tracing::trace!("Loaded: {} from {:?}", item.name, path_buf);
                            self.items.push(item);
                            count_in_dir += 1;
                        }
                        None => {
                            tracing::trace!("Skipped (parse failed or hidden): {:?}", path_buf);
                        }
                    }
                }
            }
            tracing::debug!("Loaded {} items from {:?}", count_in_dir, dir);
        }

        // Sort by name
        self.items.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

        tracing::info!("Loaded {} desktop applications total", self.items.len());

        Ok(())
    }

    fn items(&self) -> &[Item] {
        &self.items
    }

    fn activate(&self, item: &Item) -> Result<Action> {
        let exec = item.data["exec"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("No exec command for item"))?;

        Ok(Action::Launch(exec.to_string()))
    }
}

/// Clean up an Exec string by removing field codes
fn clean_exec(exec: &str) -> String {
    let mut result = String::new();
    let mut chars = exec.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '%' {
            // Skip the field code
            if let Some(&next) = chars.peek() {
                match next {
                    'f' | 'F' | 'u' | 'U' | 'd' | 'D' | 'n' | 'N' | 'i' | 'c' | 'k' | 'v' | 'm' => {
                        chars.next();
                        continue;
                    }
                    '%' => {
                        // %% -> %
                        chars.next();
                        result.push('%');
                        continue;
                    }
                    _ => {}
                }
            }
        }
        result.push(c);
    }

    // Trim trailing whitespace
    result.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_exec() {
        assert_eq!(clean_exec("firefox %u"), "firefox");
        assert_eq!(clean_exec("code %F"), "code");
        assert_eq!(clean_exec("thunar %U"), "thunar");
        assert_eq!(clean_exec("echo %%"), "echo %");
        assert_eq!(clean_exec("app --flag"), "app --flag");
    }
}
