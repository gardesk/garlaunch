use anyhow::Result;
use clap::{Parser, Subcommand};
use serde_json::json;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "garlaunchctl")]
#[command(about = "Control garlaunch")]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Show the launcher
    Show {
        /// Mode to show
        #[arg(short, long, default_value = "drun")]
        mode: String,

        /// Script source for script mode
        #[arg(short, long)]
        source: Option<String>,
    },
    /// Hide the launcher
    Hide,
    /// Toggle the launcher
    Toggle {
        /// Mode to toggle
        #[arg(short, long, default_value = "drun")]
        mode: String,
    },
    /// Get launcher status
    Status,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let request = match &args.command {
        Command::Show { mode, source } => {
            json!({
                "command": "show",
                "args": {
                    "mode": mode,
                    "source": source,
                }
            })
        }
        Command::Hide => {
            json!({
                "command": "hide"
            })
        }
        Command::Toggle { mode } => {
            json!({
                "command": "toggle",
                "args": {
                    "mode": mode,
                }
            })
        }
        Command::Status => {
            json!({
                "command": "status"
            })
        }
    };

    match send_command(&request) {
        Ok(response) => {
            if let Some(error) = response.get("error").and_then(|e| e.as_str()) {
                eprintln!("Error: {}", error);
                std::process::exit(1);
            }
            if let Some(data) = response.get("data") {
                println!("{}", serde_json::to_string_pretty(data)?);
            }
            Ok(())
        }
        Err(e) => {
            // If daemon isn't running, try to run garlaunch directly
            if e.to_string().contains("No such file") || e.to_string().contains("Connection refused") {
                match &args.command {
                    Command::Show { mode, source } => {
                        let mut cmd = std::process::Command::new("garlaunch");
                        cmd.arg("--mode").arg(mode);
                        if let Some(src) = source {
                            cmd.arg("--source").arg(src);
                        }
                        cmd.spawn()?;
                        Ok(())
                    }
                    Command::Toggle { mode } => {
                        let mut cmd = std::process::Command::new("garlaunch");
                        cmd.arg("--mode").arg(mode);
                        cmd.spawn()?;
                        Ok(())
                    }
                    _ => {
                        eprintln!("garlaunch daemon not running");
                        std::process::exit(1);
                    }
                }
            } else {
                Err(e)
            }
        }
    }
}

fn socket_path() -> PathBuf {
    std::env::var("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"))
        .join("garlaunch.sock")
}

fn send_command(request: &serde_json::Value) -> Result<serde_json::Value> {
    let socket = socket_path();
    let mut stream = UnixStream::connect(&socket)?;

    // Send request
    let request_str = serde_json::to_string(request)?;
    writeln!(stream, "{}", request_str)?;
    stream.flush()?;

    // Read response
    let mut reader = BufReader::new(stream);
    let mut response = String::new();
    reader.read_line(&mut response)?;

    let response: serde_json::Value = serde_json::from_str(&response)?;
    Ok(response)
}
