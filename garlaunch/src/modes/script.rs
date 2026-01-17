use super::{Action, Item, Mode};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

/// Script mode - runs an external script that provides items via JSON protocol
///
/// Protocol:
/// 1. garlaunch runs the script
/// 2. Script outputs JSON with items: {"items": [...]}
/// 3. User selects an item
/// 4. garlaunch sends selection to script stdin: {"selected": {...}}
/// 5. Script outputs action: {"action": "launch", "command": "..."} or {"action": "close"}
pub struct ScriptMode {
    source: PathBuf,
    items: Vec<Item>,
    child: Option<Child>,
}

/// Message from script with items
#[derive(Debug, Deserialize)]
struct ScriptItems {
    items: Vec<ScriptItem>,
}

/// Item from script
#[derive(Debug, Deserialize)]
struct ScriptItem {
    id: String,
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    icon: Option<String>,
    #[serde(default)]
    data: Option<serde_json::Value>,
}

/// Selection sent to script
#[derive(Debug, Serialize)]
struct ScriptSelection {
    selected: Item,
}

/// Action response from script
#[derive(Debug, Deserialize)]
struct ScriptAction {
    action: String,
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    data: Option<serde_json::Value>,
}

impl ScriptMode {
    pub fn new(source: PathBuf) -> Self {
        Self {
            source,
            items: Vec::new(),
            child: None,
        }
    }

    /// Send selection to script and get action
    fn send_selection(&mut self, item: &Item) -> Result<Action> {
        let child = self
            .child
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("Script not running"))?;

        let stdin = child
            .stdin
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("Script stdin not available"))?;

        // Send selection
        let selection = ScriptSelection {
            selected: item.clone(),
        };
        let json = serde_json::to_string(&selection)?;
        writeln!(stdin, "{}", json)?;
        stdin.flush()?;

        // Read action response
        let stdout = child
            .stdout
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("Script stdout not available"))?;

        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        reader.read_line(&mut line)?;

        let action: ScriptAction = serde_json::from_str(&line)
            .with_context(|| format!("Failed to parse script action: {}", line))?;

        match action.action.as_str() {
            "launch" => {
                if let Some(cmd) = action.command {
                    Ok(Action::Launch(cmd))
                } else {
                    Ok(Action::Close)
                }
            }
            "close" => Ok(Action::Close),
            "switch" => {
                if let Some(mode) = action.mode {
                    Ok(Action::SwitchMode(mode))
                } else {
                    Ok(Action::Close)
                }
            }
            "custom" => {
                if let Some(data) = action.data {
                    Ok(Action::Custom(data))
                } else {
                    Ok(Action::Close)
                }
            }
            _ => {
                tracing::warn!("Unknown script action: {}", action.action);
                Ok(Action::Close)
            }
        }
    }
}

impl Drop for ScriptMode {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
        }
    }
}

impl Mode for ScriptMode {
    fn name(&self) -> &str {
        "script"
    }

    fn load(&mut self) -> Result<()> {
        // Start the script process
        let mut child = Command::new(&self.source)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .with_context(|| format!("Failed to start script: {:?}", self.source))?;

        // Read initial items from stdout
        let stdout = child
            .stdout
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("Script stdout not available"))?;

        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        reader.read_line(&mut line)?;

        let script_items: ScriptItems = serde_json::from_str(&line)
            .with_context(|| format!("Failed to parse script items: {}", line))?;

        self.items = script_items
            .items
            .into_iter()
            .map(|si| {
                let mut item = Item::new(si.id, si.name);
                if let Some(desc) = si.description {
                    item = item.with_description(desc);
                }
                if let Some(icon) = si.icon {
                    item = item.with_icon(icon);
                }
                if let Some(data) = si.data {
                    item = item.with_data(data);
                }
                item
            })
            .collect();

        self.child = Some(child);

        tracing::info!(
            "Loaded {} items from script {:?}",
            self.items.len(),
            self.source
        );
        Ok(())
    }

    fn items(&self) -> &[Item] {
        &self.items
    }

    fn activate(&self, item: &Item) -> Result<Action> {
        // We need mutable access to send selection
        // This is a bit awkward with the trait design, so we'll create a new process
        // In a real implementation, we'd want to refactor the trait

        // For now, just return a custom action with the item data
        // The caller can handle it appropriately
        Ok(Action::Custom(serde_json::json!({
            "script": self.source.to_string_lossy(),
            "item": item,
        })))
    }
}
