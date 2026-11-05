#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod apps;
mod calculator;
mod config;
mod coordinator;
mod hotkey;
mod ipc;
mod layout;
mod system;
mod tray;
mod updater;

use app::JLauncherApp;
use ipc::{send_command, start_server, IpcCommand};
use log::info;
use std::env;
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::mpsc::channel;

fn print_help() {
    println!(
        r#"JLauncher - Fast, lightweight keyboard launcher for Linux and Windows

Usage:
    jlauncher [OPTIONS]

Options:
    --toggle       Toggle launcher window visibility (send to running instance or start)
    --show         Show launcher window
    --hide         Hide launcher window
    --settings     Open settings dialog
    --quit         Quit running instance
    --hidden       Start in background (system tray only, no window)
    --help, -h     Print this help message

Wayland Note:
    Under Wayland compositors (GNOME, KDE Plasma, Sway, Hyprland), bind your preferred
    global hotkey in Desktop Environment Settings to run:
        jlauncher --toggle
"#
    );
}

fn init_logger() {
    use std::fs::OpenOptions;
    use std::io::Write;

    if let Some(proj) = directories::ProjectDirs::from("org", "jlauncher", "jlauncher") {
        let dir = proj.data_local_dir();
        let _ = std::fs::create_dir_all(dir);
        let log_file = dir.join("jlauncher.log");
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(log_file) {
            let _ = writeln!(
                file,
                "--- JLauncher starting at {:?} ---",
                std::time::SystemTime::now()
            );
        }
    }

    std::panic::set_hook(Box::new(|info| {
        log::error!("PANIC: {:?}", info);
        if let Some(proj) = directories::ProjectDirs::from("org", "jlauncher", "jlauncher") {
            let log_file = proj.data_local_dir().join("jlauncher.log");
            if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(log_file) {
                let _ = writeln!(f, "[PANIC] {:?}", info);
            }
        }
    }));

    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format(|buf, record| {
            use std::io::Write;
            let ts = buf.timestamp();
            let line = format!(
                "[{} {} {}] {}",
                ts,
                record.level(),
                record.target(),
                record.args()
            );
            if let Some(proj) = directories::ProjectDirs::from("org", "jlauncher", "jlauncher") {
                let log_file = proj.data_local_dir().join("jlauncher.log");
                if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(log_file) {
                    let _ = writeln!(f, "{}", line);
                }
            }
            writeln!(buf, "{}", line)
        })
        .try_init();
}

#[cfg(target_os = "windows")]
fn ensure_interactive_desktop() {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::System::StationsAndDesktops::{
        GetThreadDesktop, GetUserObjectInformationW, UOI_NAME,
    };
    use windows_sys::Win32::System::Threading::{
        CreateProcessW, GetCurrentThreadId, PROCESS_INFORMATION, STARTUPINFOW,
    };

    unsafe {
        let hdesk = GetThreadDesktop(GetCurrentThreadId());
        let mut name_buf = [0u16; 256];
        let mut len_needed = 0;
        let ok = GetUserObjectInformationW(
            hdesk,
            UOI_NAME as i32,
            name_buf.as_mut_ptr() as _,
            (name_buf.len() * 2) as u32,
            &mut len_needed,
        );

        let desk_name = if ok != 0 {
            let nul_idx = name_buf
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(name_buf.len());
            String::from_utf16_lossy(&name_buf[..nul_idx])
        } else {
            String::new()
        };

        info!("Current thread desktop: '{}'", desk_name);

        if !desk_name.is_empty() && !desk_name.eq_ignore_ascii_case("Default") {
            info!("Not on interactive Default desktop (current: '{}')! Respawning on WinSta0\\Default...", desk_name);

            let Ok(exe_path) = std::env::current_exe() else {
                return;
            };

            let mut exe_str = exe_path.to_string_lossy().to_string();
            if let Some(stripped) = exe_str.strip_prefix(r"\\?\") {
                exe_str = stripped.to_string();
            }

            let args: Vec<String> = std::env::args().collect();
            let mut cmd_parts = vec![format!("\"{}\"", exe_str)];
            for arg in args.into_iter().skip(1) {
                if arg.contains(' ') {
                    cmd_parts.push(format!("\"{}\"", arg));
                } else {
                    cmd_parts.push(arg);
                }
            }
            let full_cmd = cmd_parts.join(" ");

            let _app_name_wide: Vec<u16> = OsStr::new(&exe_str)
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();

            let mut cmd_wide: Vec<u16> = OsStr::new(&full_cmd)
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();

            let desktop_str: Vec<u16> = OsStr::new("WinSta0\\Default")
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();

            let mut si: STARTUPINFOW = std::mem::zeroed();
            si.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
            si.lpDesktop = desktop_str.as_ptr() as *mut u16;

            let mut pi: PROCESS_INFORMATION = std::mem::zeroed();

            const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x01000000;
            let mut creation_flags = CREATE_BREAKAWAY_FROM_JOB;

            let mut spawned = CreateProcessW(
                std::ptr::null(),
                cmd_wide.as_mut_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                0,
                creation_flags,
                std::ptr::null(),
                std::ptr::null(),
                &si,
                &mut pi,
            );

            if spawned == 0 {
                // If breakaway was not allowed by the job, retry without it
                creation_flags = 0;
                spawned = CreateProcessW(
                    std::ptr::null(),
                    cmd_wide.as_mut_ptr(),
                    std::ptr::null(),
                    std::ptr::null(),
                    0,
                    creation_flags,
                    std::ptr::null(),
                    std::ptr::null(),
                    &si,
                    &mut pi,
                );
            }

            if spawned != 0 {
                info!("Successfully respawned JLauncher on WinSta0\\Default (PID {}). Forwarding execution to child.", pi.dwProcessId);
                windows_sys::Win32::Foundation::CloseHandle(pi.hThread);
                windows_sys::Win32::System::Threading::WaitForSingleObject(
                    pi.hProcess,
                    windows_sys::Win32::System::Threading::INFINITE,
                );
                let mut exit_code: u32 = 0;
                windows_sys::Win32::System::Threading::GetExitCodeProcess(
                    pi.hProcess,
                    &mut exit_code,
                );
                info!(
                    "Child process (PID {}) finished with exit code {}.",
                    pi.dwProcessId, exit_code
                );
                windows_sys::Win32::Foundation::CloseHandle(pi.hProcess);
                std::process::exit(exit_code as i32);
            } else {
                log::warn!(
                    "Failed to spawn on WinSta0\\Default: error {}",
                    std::io::Error::last_os_error()
                );
            }
        }
    }
}

fn main() -> eframe::Result<()> {
    #[cfg(target_os = "linux")]
    {
        // When running under Linux desktop, if DISPLAY is present and user didn't explicitly
        // specify JLAUNCHER_BACKEND=wayland, prefer X11 (via Xwayland or native X11).
        // This is essential because Wayland xdg-shell does not support hiding windows
        // (set_visible is a no-op on Wayland in winit), does not support global hotkeys,
        // and causes frame callback hangs when unmapped.
        if std::env::var("JLAUNCHER_BACKEND")
            .map(|v| v != "wayland")
            .unwrap_or(true)
            && std::env::var("DISPLAY").is_ok()
        {
            std::env::remove_var("WAYLAND_DISPLAY");
            std::env::remove_var("WAYLAND_SOCKET");
        }
    }

    init_logger();

    let args: Vec<String> = env::args().collect();
    let mut start_hidden = false;

    if args.len() > 1 {
        let first_arg = args[1].as_str();
        match first_arg {
            "--help" | "-h" | "help" => {
                print_help();
                return Ok(());
            }
            "--toggle" | "toggle" => {
                if send_command(IpcCommand::Toggle).is_ok() {
                    info!("Toggled existing JLauncher instance");
                    return Ok(());
                }
                // Not running yet, start visible
            }
            "--show" | "show" => {
                if send_command(IpcCommand::Show).is_ok() {
                    info!("Showed existing JLauncher instance");
                    return Ok(());
                }
            }
            "--hide" | "hide" => {
                if send_command(IpcCommand::Hide).is_ok() {
                    info!("Hidden existing JLauncher instance");
                    return Ok(());
                }
                return Ok(());
            }
            "--settings" | "settings" => {
                if send_command(IpcCommand::Settings).is_ok() {
                    info!("Opened settings in existing JLauncher instance");
                    return Ok(());
                }
            }
            "--quit" | "quit" => {
                if send_command(IpcCommand::Quit).is_ok() {
                    info!("Sent quit to existing JLauncher instance");
                    return Ok(());
                }
                return Ok(());
            }
            "--hidden" | "hidden" => {
                start_hidden = true;
            }
            _ => {}
        }
    }

    #[cfg(target_os = "windows")]
    ensure_interactive_desktop();

    // Channel for IPC commands received from CLI or other instances
    let (ipc_tx, ipc_rx) = channel();

    // Start single instance IPC listener
    if let Err(e) = start_server(ipc_tx, None) {
        // Port already bound -> another instance is already running!
        info!("Another JLauncher instance is running: {}", e);
        let _ = send_command(IpcCommand::Toggle);
        return Ok(());
    }

    info!("Starting JLauncher...");

    let (screen_w, screen_h) = coordinator::get_screen_size();
    info!("Screen dimensions for overlay: {}x{}", screen_w, screen_h);

    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("JLauncher")
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top()
            .with_position([0.0, 0.0])
            .with_inner_size([screen_w, screen_h])
            .with_resizable(true)
            .with_visible(!start_hidden)
            .with_active(true),
        ..Default::default()
    };

    info!("Calling eframe::run_native...");
    let res = eframe::run_native(
        "JLauncher",
        native_options,
        Box::new(move |cc| {
            info!("In eframe creation closure");
            Ok(Box::new(JLauncherApp::new(cc, ipc_rx, start_hidden)))
        }),
    );

    if let Some(proj) = directories::ProjectDirs::from("org", "jlauncher", "jlauncher") {
        let log_file = proj.data_local_dir().join("jlauncher.log");
        if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(log_file) {
            let _ = writeln!(f, "[EXIT] eframe::run_native returned: {:?}", res);
        }
    }

    info!("eframe::run_native finished with: {:?}", res);
    std::process::exit(0);
}
