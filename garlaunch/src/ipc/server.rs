use super::{socket_path, Request, Response};
use anyhow::Result;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

/// Commands that can be sent to the launcher via IPC
#[derive(Debug, Clone)]
pub enum IpcCommand {
    /// Show the launcher with the given mode
    Show { mode: String, source: Option<String> },
    /// Hide the launcher
    Hide,
    /// Toggle visibility
    Toggle { mode: String },
    /// Query status
    Status,
}

/// IPC Server for garlaunch daemon mode
pub struct IpcServer {
    listener: UnixListener,
    command_tx: Sender<IpcCommand>,
    command_rx: Receiver<IpcCommand>,
}

impl IpcServer {
    /// Create a new IPC server
    pub fn new() -> Result<Self> {
        let path = socket_path();

        // Remove existing socket
        if path.exists() {
            std::fs::remove_file(&path)?;
        }

        let listener = UnixListener::bind(&path)?;
        listener.set_nonblocking(true)?;

        let (command_tx, command_rx) = mpsc::channel();

        tracing::info!("IPC server listening on {:?}", path);

        Ok(Self {
            listener,
            command_tx,
            command_rx,
        })
    }

    /// Get the command receiver for the main loop to poll
    pub fn command_receiver(&self) -> &Receiver<IpcCommand> {
        &self.command_rx
    }

    /// Poll for incoming connections (non-blocking)
    pub fn poll(&self) -> Result<()> {
        match self.listener.accept() {
            Ok((stream, _)) => {
                let tx = self.command_tx.clone();
                thread::spawn(move || {
                    if let Err(e) = handle_client(stream, tx) {
                        tracing::error!("Client handler error: {}", e);
                    }
                });
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // No connections pending
            }
            Err(e) => {
                tracing::error!("Accept error: {}", e);
            }
        }
        Ok(())
    }

    /// Get the socket path
    pub fn socket_path(&self) -> std::path::PathBuf {
        socket_path()
    }
}

impl Drop for IpcServer {
    fn drop(&mut self) {
        let path = socket_path();
        let _ = std::fs::remove_file(path);
    }
}

fn handle_client(mut stream: UnixStream, tx: Sender<IpcCommand>) -> Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();

    reader.read_line(&mut line)?;

    let request: Request = match serde_json::from_str(&line) {
        Ok(req) => req,
        Err(e) => {
            let response = Response::error(format!("Invalid request: {}", e));
            writeln!(stream, "{}", serde_json::to_string(&response)?)?;
            return Ok(());
        }
    };

    let response = match request.command.as_str() {
        "show" => {
            let mode = request.args["mode"]
                .as_str()
                .unwrap_or("drun")
                .to_string();
            let source = request.args["source"].as_str().map(String::from);

            tx.send(IpcCommand::Show { mode, source })?;
            Response::ok()
        }
        "hide" => {
            tx.send(IpcCommand::Hide)?;
            Response::ok()
        }
        "toggle" => {
            let mode = request.args["mode"]
                .as_str()
                .unwrap_or("drun")
                .to_string();

            tx.send(IpcCommand::Toggle { mode })?;
            Response::ok()
        }
        "status" => {
            tx.send(IpcCommand::Status)?;
            Response::ok_with_data(serde_json::json!({
                "running": true,
            }))
        }
        _ => Response::error(format!("Unknown command: {}", request.command)),
    };

    writeln!(stream, "{}", serde_json::to_string(&response)?)?;
    stream.flush()?;

    Ok(())
}
