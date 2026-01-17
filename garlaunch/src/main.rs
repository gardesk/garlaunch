mod app;
mod config;
mod frecency;
mod ipc;
mod modes;
mod search;
mod ui;

use anyhow::Result;
use clap::Parser;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[derive(Parser, Debug)]
#[command(name = "garlaunch")]
#[command(about = "A rofi-like application launcher")]
struct Args {
    /// Mode to launch in
    #[arg(short, long, default_value = "drun")]
    mode: String,

    /// Custom prompt text
    #[arg(short, long)]
    prompt: Option<String>,

    /// Script source for script mode
    #[arg(short, long)]
    source: Option<String>,

    /// Run as daemon (listen for IPC commands)
    #[arg(short, long)]
    daemon: bool,
}

fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let args = Args::parse();

    tracing::info!("Starting garlaunch in {} mode", args.mode);

    if args.daemon {
        run_daemon()
    } else {
        run_launcher(&args)
    }
}

fn run_daemon() -> Result<()> {
    use ipc::{IpcCommand, IpcServer};
    use std::time::Duration;

    tracing::info!("Running in daemon mode");

    let server = IpcServer::new()?;
    let command_rx = server.command_receiver();

    tracing::info!("Daemon listening on {:?}", server.socket_path());

    // Simple event loop - poll for IPC commands
    loop {
        // Poll for new connections
        server.poll()?;

        // Check for commands (non-blocking)
        match command_rx.try_recv() {
            Ok(cmd) => {
                tracing::info!("Received command: {:?}", cmd);
                match cmd {
                    IpcCommand::Show { mode, source } => {
                        // Spawn launcher in a separate process
                        let mut cmd = std::process::Command::new(std::env::current_exe()?);
                        cmd.arg("--mode").arg(&mode);
                        if let Some(src) = source {
                            cmd.arg("--source").arg(src);
                        }
                        let _ = cmd.spawn();
                    }
                    IpcCommand::Hide => {
                        // TODO: Send hide signal to running launcher
                        tracing::info!("Hide command received (not yet implemented)");
                    }
                    IpcCommand::Toggle { mode } => {
                        // TODO: Toggle launcher visibility
                        // For now, just spawn it
                        let mut cmd = std::process::Command::new(std::env::current_exe()?);
                        cmd.arg("--mode").arg(&mode);
                        let _ = cmd.spawn();
                    }
                    IpcCommand::Status => {
                        // Status is handled in the server response
                    }
                }
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                // No commands, sleep briefly
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                tracing::error!("Command channel disconnected");
                break;
            }
        }
    }

    Ok(())
}

fn run_launcher(args: &Args) -> Result<()> {
    // Validate script mode requires source
    if args.mode == "script" && args.source.is_none() {
        anyhow::bail!("Script mode requires --source argument");
    }

    // Initialize app
    let mut app = app::App::new(
        &args.mode,
        args.prompt.as_deref(),
        args.source.as_ref().map(std::path::PathBuf::from),
    )?;

    // Run the app
    app.run()?;

    // If an item was selected, execute it
    if let Some(action) = app.take_result() {
        action.execute()?;
    }

    Ok(())
}
