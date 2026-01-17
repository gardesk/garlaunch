use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

/// Client for communicating with the gar window manager
pub struct GarClient {
    stream: Option<UnixStream>,
}

/// Window information from gar
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowInfo {
    pub id: u32,
    pub title: String,
    pub class: Option<String>,
    pub instance: Option<String>,
    pub workspace: u32,
    pub focused: bool,
}

/// Request to gar IPC
#[derive(Debug, Serialize)]
struct GarRequest {
    command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    args: Option<serde_json::Value>,
}

/// Response from gar IPC
#[derive(Debug, Deserialize)]
struct GarResponse {
    success: bool,
    #[serde(default)]
    data: Option<serde_json::Value>,
    #[serde(default)]
    error: Option<String>,
}

impl GarClient {
    /// Create a new client (does not connect yet)
    pub fn new() -> Self {
        Self { stream: None }
    }

    /// Get the gar socket path
    fn socket_path() -> PathBuf {
        std::env::var("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/tmp"))
            .join("gar.sock")
    }

    /// Check if gar is running (socket exists)
    pub fn is_available() -> bool {
        Self::socket_path().exists()
    }

    /// Connect to gar IPC
    pub fn connect(&mut self) -> Result<()> {
        let path = Self::socket_path();

        if !path.exists() {
            anyhow::bail!("gar socket not found at {:?}", path);
        }

        let stream = UnixStream::connect(&path)
            .with_context(|| format!("Failed to connect to gar at {:?}", path))?;

        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        stream.set_write_timeout(Some(Duration::from_secs(5)))?;

        self.stream = Some(stream);
        Ok(())
    }

    /// Disconnect from gar
    pub fn disconnect(&mut self) {
        self.stream = None;
    }

    /// Send a request and get a response
    fn request(&mut self, command: &str, args: Option<serde_json::Value>) -> Result<GarResponse> {
        let stream = self
            .stream
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("Not connected to gar"))?;

        let request = GarRequest {
            command: command.to_string(),
            args,
        };

        // Send request
        let json = serde_json::to_string(&request)?;
        writeln!(stream, "{}", json)?;
        stream.flush()?;

        // Read response
        let mut reader = BufReader::new(stream.try_clone()?);
        let mut line = String::new();
        reader.read_line(&mut line)?;

        let response: GarResponse = serde_json::from_str(&line)
            .with_context(|| format!("Failed to parse gar response: {}", line))?;

        if !response.success {
            if let Some(error) = &response.error {
                anyhow::bail!("gar error: {}", error);
            }
        }

        Ok(response)
    }

    /// Get list of all windows
    pub fn get_windows(&mut self) -> Result<Vec<WindowInfo>> {
        let response = self.request("get_windows", None)?;

        if let Some(data) = response.data {
            let windows: Vec<WindowInfo> = serde_json::from_value(data)
                .context("Failed to parse window list")?;
            Ok(windows)
        } else {
            Ok(Vec::new())
        }
    }

    /// Focus a specific window by ID
    pub fn focus_window(&mut self, window_id: u32) -> Result<()> {
        self.request(
            "focus_window",
            Some(serde_json::json!({ "id": window_id })),
        )?;
        Ok(())
    }

    /// Get the currently focused window
    pub fn get_focused_window(&mut self) -> Result<Option<WindowInfo>> {
        let windows = self.get_windows()?;
        Ok(windows.into_iter().find(|w| w.focused))
    }

    /// Get windows on a specific workspace
    pub fn get_windows_on_workspace(&mut self, workspace: u32) -> Result<Vec<WindowInfo>> {
        let windows = self.get_windows()?;
        Ok(windows.into_iter().filter(|w| w.workspace == workspace).collect())
    }
}

impl Default for GarClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_socket_path() {
        let path = GarClient::socket_path();
        assert!(path.to_string_lossy().contains("gar.sock"));
    }

    #[test]
    fn test_is_available_when_not_running() {
        // This test assumes gar is not running in the test environment
        // It's more of a sanity check
        let _ = GarClient::is_available();
    }
}
