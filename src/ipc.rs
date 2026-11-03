use eframe::egui;
use log::{error, info};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;

pub const IPC_PORT: u16 = 41789;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IpcCommand {
    Toggle,
    Show,
    Hide,
    Settings,
    Quit,
}

impl IpcCommand {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_uppercase().as_str() {
            "TOGGLE" => Some(Self::Toggle),
            "SHOW" => Some(Self::Show),
            "HIDE" => Some(Self::Hide),
            "SETTINGS" => Some(Self::Settings),
            "QUIT" => Some(Self::Quit),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Toggle => "TOGGLE",
            Self::Show => "SHOW",
            Self::Hide => "HIDE",
            Self::Settings => "SETTINGS",
            Self::Quit => "QUIT",
        }
    }
}

pub fn send_command(cmd: IpcCommand) -> Result<(), String> {
    let addr = format!("127.0.0.1:{}", IPC_PORT);
    let mut stream =
        TcpStream::connect_timeout(&addr.parse().unwrap(), Duration::from_millis(1000))
            .map_err(|e| format!("Could not connect to running instance: {}", e))?;

    let _ = stream.set_write_timeout(Some(Duration::from_millis(1000)));
    let msg = format!("{}\n", cmd.as_str());
    stream
        .write_all(msg.as_bytes())
        .map_err(|e| e.to_string())?;
    stream.flush().map_err(|e| e.to_string())?;
    let _ = stream.shutdown(std::net::Shutdown::Both);
    Ok(())
}

pub fn start_server(_sender: Sender<IpcCommand>, ctx: Option<egui::Context>) -> Result<(), String> {
    let addr = format!("127.0.0.1:{}", IPC_PORT);
    let listener = TcpListener::bind(&addr)
        .map_err(|e| format!("Failed to bind IPC port {}: {}", IPC_PORT, e))?;

    thread::spawn(move || {
        info!("IPC server listening on 127.0.0.1:{}", IPC_PORT);
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    let ctx_clone = ctx.clone();
                    thread::spawn(move || {
                        let _ = stream.set_read_timeout(Some(Duration::from_millis(800)));
                        let mut reader = BufReader::new(stream);
                        let mut line = String::new();
                        if let Ok(_) = reader.read_line(&mut line) {
                            if let Some(cmd) = IpcCommand::from_str(&line) {
                                info!("IPC server received command: {:?}", cmd);
                                match cmd {
                                    IpcCommand::Toggle => crate::coordinator::toggle_window(),
                                    IpcCommand::Show => crate::coordinator::show_window(false),
                                    IpcCommand::Hide => crate::coordinator::hide_window(),
                                    IpcCommand::Settings => crate::coordinator::show_window(true),
                                    IpcCommand::Quit => crate::coordinator::quit_app(),
                                }
                                if let Some(c) = &ctx_clone {
                                    c.request_repaint();
                                }
                            }
                        }
                    });
                }
                Err(e) => {
                    error!("IPC stream error: {:?}", e);
                }
            }
        }
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ipc_commands_parsing() {
        assert_eq!(IpcCommand::from_str("toggle"), Some(IpcCommand::Toggle));
        assert_eq!(IpcCommand::from_str("SHOW\n"), Some(IpcCommand::Show));
        assert_eq!(IpcCommand::from_str("hide"), Some(IpcCommand::Hide));
        assert_eq!(IpcCommand::from_str("settings"), Some(IpcCommand::Settings));
        assert_eq!(IpcCommand::from_str("quit"), Some(IpcCommand::Quit));
        assert_eq!(IpcCommand::from_str("unknown_cmd"), None);
    }
}
