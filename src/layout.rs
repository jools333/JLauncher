//! Keyboard layout detection and switching.
//! Supports Linux (IBus, GNOME, KDE, Hyprland, Sway, X11 setxkbmap) and Windows.

#[allow(unused_imports)]
use log::{debug, info, warn};

pub fn ensure_english_layout() {
    #[cfg(target_os = "linux")]
    ensure_english_layout_linux();

    #[cfg(target_os = "windows")]
    ensure_english_layout_windows();
}

#[cfg(target_os = "linux")]
fn ensure_english_layout_linux() {
    // 1. Try IBus (standard for GNOME, Ubuntu, Fedora)
    if try_ibus_switch_to_english() {
        return;
    }

    // 2. Try KDE Plasma DBus
    if try_kde_switch_to_english() {
        return;
    }

    // 3. Try Hyprland
    if try_hyprland_switch_to_english() {
        return;
    }

    // 4. Try Sway
    if try_sway_switch_to_english() {
        return;
    }

    // 5. Try standard X11 setxkbmap
    if try_setxkbmap_switch_to_english() {
        return;
    }
}

#[cfg(target_os = "linux")]
fn try_ibus_switch_to_english() -> bool {
    // Find active ibus socket in ~/.cache/ibus/dbus-*
    let cache_dir = match std::env::var("HOME") {
        Ok(h) => std::path::PathBuf::from(h).join(".cache").join("ibus"),
        Err(_) => return false,
    };

    if !cache_dir.is_dir() {
        return false;
    }

    let mut newest_socket: Option<(std::path::PathBuf, std::time::SystemTime)> = None;
    if let Ok(entries) = std::fs::read_dir(&cache_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                if file_name.starts_with("dbus-") {
                    if let Ok(metadata) = entry.metadata() {
                        if let Ok(modified) = metadata.modified() {
                            if newest_socket.as_ref().map_or(true, |(_, m)| modified > *m) {
                                newest_socket = Some((path, modified));
                            }
                        }
                    }
                }
            }
        }
    }

    let socket_path = match newest_socket {
        Some((p, _)) => p,
        None => return false,
    };

    let ibus_addr = format!("unix:path={}", socket_path.display());

    // Check current layout via `ibus engine`
    let query_output = std::process::Command::new("ibus")
        .arg("engine")
        .env("IBUS_ADDRESS", &ibus_addr)
        .output();

    let output = match query_output {
        Ok(out) if out.status.success() => {
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        }
        _ => return false,
    };

    if is_english_ibus_engine(&output) {
        debug!("IBus layout is already English ('{}')", output);
        return true;
    }

    info!("Current IBus layout is not English ('{}'), switching to 'xkb:us::eng'", output);

    // Switch to English engine
    let switch_status = std::process::Command::new("ibus")
        .args(["engine", "xkb:us::eng"])
        .env("IBUS_ADDRESS", &ibus_addr)
        .status();

    match switch_status {
        Ok(status) if status.success() => {
            info!("Successfully switched IBus layout to English ('xkb:us::eng')");
            true
        }
        _ => false,
    }
}

#[cfg(target_os = "linux")]
fn is_english_ibus_engine(engine_str: &str) -> bool {
    let parts: Vec<&str> = engine_str.trim().split(':').collect();
    if parts.len() >= 2 && parts[0] == "xkb" {
        let layout = parts[1].to_ascii_lowercase();
        if layout == "us" || layout == "en" || layout == "uk" || layout == "gb" {
            return true;
        }
        if parts.len() >= 4 {
            let lang = parts[3].to_ascii_lowercase();
            if lang == "eng" || lang == "en" {
                return true;
            }
        }
    }
    false
}

#[cfg(target_os = "linux")]
fn try_kde_switch_to_english() -> bool {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default().to_ascii_uppercase();
    if !desktop.contains("KDE") {
        return false;
    }

    let res = std::process::Command::new("qdbus")
        .args(["org.kde.keyboard", "/Layouts", "setLayout", "0"])
        .status();

    match res {
        Ok(s) if s.success() => {
            info!("Successfully switched KDE layout to index 0");
            true
        }
        _ => false,
    }
}

#[cfg(target_os = "linux")]
fn try_hyprland_switch_to_english() -> bool {
    if std::env::var("HYPRLAND_INSTANCE_SIGNATURE").is_err() {
        return false;
    }

    let res = std::process::Command::new("hyprctl")
        .args(["switchxkblayout", "all", "0"])
        .status();

    match res {
        Ok(s) if s.success() => {
            info!("Successfully switched Hyprland layout to index 0");
            true
        }
        _ => false,
    }
}

#[cfg(target_os = "linux")]
fn try_sway_switch_to_english() -> bool {
    if std::env::var("SWAYSOCK").is_err() {
        return false;
    }

    let res = std::process::Command::new("swaymsg")
        .args(["input", "*", "xkb_switch_layout", "0"])
        .status();

    match res {
        Ok(s) if s.success() => {
            info!("Successfully switched Sway layout to index 0");
            true
        }
        _ => false,
    }
}

#[cfg(target_os = "linux")]
fn try_setxkbmap_switch_to_english() -> bool {
    let query = std::process::Command::new("setxkbmap")
        .arg("-query")
        .output();

    if let Ok(out) = query {
        if out.status.success() {
            let stdout = String::from_utf8_lossy(&out.stdout);
            for line in stdout.lines() {
                if let Some(layout_part) = line.strip_prefix("layout:") {
                    let layout = layout_part.trim();
                    if layout.starts_with("us") || layout.starts_with("en") {
                        debug!("setxkbmap layout is already English ('{}')", layout);
                        return true;
                    }
                }
            }
        }
    }

    let res = std::process::Command::new("setxkbmap")
        .args(["-layout", "us"])
        .status();

    match res {
        Ok(s) if s.success() => {
            info!("Switched layout via setxkbmap to 'us'");
            true
        }
        _ => false,
    }
}

#[cfg(target_os = "windows")]
fn ensure_english_layout_windows() {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetKeyboardLayout, ActivateKeyboardLayout, LoadKeyboardLayoutW, KLF_ACTIVATE, KLF_SETFORPROCESS,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        PostMessageW, WM_INPUTLANGCHANGEREQUEST,
    };

    unsafe {
        let current_hkl = GetKeyboardLayout(0) as usize;
        let lang_id = (current_hkl & 0xFFFF) as u16;
        let primary_lang = lang_id & 0x03FF;
        const LANG_ENGLISH: u16 = 0x09;

        if primary_lang == LANG_ENGLISH {
            debug!("Keyboard layout is already English (LANGID: 0x{:04X})", lang_id);
            return;
        }

        info!("Current keyboard layout is not English (0x{:04X}), switching to English (00000409)", lang_id);

        let wide_en: Vec<u16> = "00000409\0".encode_utf16().collect();
        let en_hkl = LoadKeyboardLayoutW(wide_en.as_ptr(), KLF_ACTIVATE);
        if !en_hkl.is_null() {
            ActivateKeyboardLayout(en_hkl, KLF_SETFORPROCESS);

            let hwnd = crate::coordinator::get_main_hwnd();
            if hwnd != 0 {
                PostMessageW(
                    hwnd as _,
                    WM_INPUTLANGCHANGEREQUEST,
                    0,
                    en_hkl as isize,
                );
            }
        }
    }
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
pub fn ensure_english_layout() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "linux")]
    fn test_is_english_ibus_engine() {
        assert!(is_english_ibus_engine("xkb:us::eng"));
        assert!(is_english_ibus_engine("xkb:gb::eng"));
        assert!(is_english_ibus_engine("xkb:us:alt-intl:eng"));
        assert!(!is_english_ibus_engine("xkb:ru::rus"));
        assert!(!is_english_ibus_engine("xkb:de::ger"));
        assert!(!is_english_ibus_engine("xkb:fr::fra"));
        assert!(!is_english_ibus_engine("libpinyin"));
    }
}
