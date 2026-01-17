use super::{Action, Item, Mode};
use crate::ipc::GarClient;
use anyhow::Result;

/// Window switcher mode - shows windows from gar WM
pub struct WindowMode {
    items: Vec<Item>,
    client: GarClient,
}

impl WindowMode {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            client: GarClient::new(),
        }
    }
}

impl Default for WindowMode {
    fn default() -> Self {
        Self::new()
    }
}

impl Mode for WindowMode {
    fn name(&self) -> &str {
        "window"
    }

    fn load(&mut self) -> Result<()> {
        // Check if gar is available
        if !GarClient::is_available() {
            tracing::warn!("gar is not running, window mode unavailable");
            return Ok(());
        }

        // Connect to gar
        self.client.connect()?;

        // Get window list
        let windows = self.client.get_windows()?;

        self.items = windows
            .into_iter()
            .map(|w| {
                let desc = match (&w.class, &w.instance) {
                    (Some(class), Some(instance)) if class != instance => {
                        format!("{} - {} [WS {}]", class, instance, w.workspace)
                    }
                    (Some(class), _) => format!("{} [WS {}]", class, w.workspace),
                    _ => format!("Window {} [WS {}]", w.id, w.workspace),
                };

                Item::new(w.id.to_string(), &w.title)
                    .with_description(desc)
                    .with_data(serde_json::json!({
                        "window_id": w.id,
                        "workspace": w.workspace,
                        "focused": w.focused,
                    }))
            })
            .collect();

        tracing::info!("Loaded {} windows", self.items.len());
        Ok(())
    }

    fn items(&self) -> &[Item] {
        &self.items
    }

    fn activate(&self, item: &Item) -> Result<Action> {
        // Get window ID from item data
        let window_id = item.data["window_id"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("Invalid window ID"))? as u32;

        // Create a new client to focus the window
        // (we can't use self.client because activate takes &self)
        let mut client = GarClient::new();
        if client.connect().is_ok() {
            if let Err(e) = client.focus_window(window_id) {
                tracing::error!("Failed to focus window {}: {}", window_id, e);
            }
        }

        Ok(Action::Close)
    }
}
