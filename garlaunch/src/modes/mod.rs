mod drun;
mod run;
mod script;
mod window;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Command;

pub use drun::DrunMode;
pub use run::RunMode;
pub use script::ScriptMode;
pub use window::WindowMode;

/// An item that can be displayed and selected
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Item {
    /// Unique identifier
    pub id: String,
    /// Display name
    pub name: String,
    /// Optional description
    pub description: Option<String>,
    /// Optional icon name
    pub icon: Option<String>,
    /// Additional data (mode-specific)
    #[serde(default)]
    pub data: serde_json::Value,
}

impl Item {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: None,
            icon: None,
            data: serde_json::Value::Null,
        }
    }

    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    pub fn with_icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = data;
        self
    }
}

/// Action to take after selecting an item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Action {
    /// Close the launcher
    Close,
    /// Launch a command
    Launch(String),
    /// Switch to another mode
    SwitchMode(String),
    /// Custom action (for script mode)
    Custom(serde_json::Value),
}

impl Action {
    /// Execute the action
    pub fn execute(&self) -> Result<()> {
        match self {
            Action::Launch(cmd) => {
                tracing::info!("Launching: {}", cmd);
                // Use shell to handle complex commands
                Command::new("sh")
                    .arg("-c")
                    .arg(cmd)
                    .spawn()?;
                Ok(())
            }
            Action::Close => Ok(()),
            Action::SwitchMode(_) => Ok(()),
            Action::Custom(data) => {
                tracing::info!("Custom action: {:?}", data);
                Ok(())
            }
        }
    }
}

/// A mode provides items and handles selection
pub trait Mode: Send {
    /// Get the mode name
    fn name(&self) -> &str;

    /// Load items (called once on startup)
    fn load(&mut self) -> Result<()>;

    /// Get all items
    fn items(&self) -> &[Item];

    /// Handle item activation
    fn activate(&self, item: &Item) -> Result<Action>;
}

/// Create a mode by name
pub fn create_mode(name: &str) -> Result<Box<dyn Mode>> {
    match name {
        "drun" => Ok(Box::new(DrunMode::new())),
        "run" => Ok(Box::new(RunMode::new())),
        "window" => Ok(Box::new(WindowMode::new())),
        _ => anyhow::bail!("Unknown mode: {}", name),
    }
}

/// Create a script mode with the given source
pub fn create_script_mode(source: PathBuf) -> Box<dyn Mode> {
    Box::new(ScriptMode::new(source))
}
