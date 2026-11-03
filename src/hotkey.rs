use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::GlobalHotKeyManager;
use log::{error, info};

pub struct HotkeyService {
    manager: Option<GlobalHotKeyManager>,
    current_hotkey: Option<HotKey>,
}

impl HotkeyService {
    pub fn new() -> Self {
        let manager = match GlobalHotKeyManager::new() {
            Ok(m) => Some(m),
            Err(e) => {
                error!("Failed to initialize GlobalHotKeyManager: {:?}", e);
                None
            }
        };

        Self {
            manager,
            current_hotkey: None,
        }
    }

    pub fn register(&mut self, shortcut_str: &str) -> Result<HotKey, String> {
        let manager = self.manager.as_mut().ok_or("HotKeyManager not available")?;

        let hotkey = parse_hotkey(shortcut_str)
            .ok_or_else(|| format!("Invalid shortcut format: '{}'", shortcut_str))?;

        // Unregister previous hotkey if any
        if let Some(prev) = self.current_hotkey.take() {
            let _ = manager.unregister(prev);
        }

        manager
            .register(hotkey)
            .map_err(|e| format!("Failed to register hotkey '{}': {:?}", shortcut_str, e))?;

        #[cfg(target_os = "linux")]
        sync_gnome_hotkey(shortcut_str);

        info!("Registered global hotkey: {}", shortcut_str);
        self.current_hotkey = Some(hotkey);
        Ok(hotkey)
    }

    #[allow(dead_code)]
    pub fn current_id(&self) -> Option<u32> {
        self.current_hotkey.map(|h| h.id())
    }
}

pub fn parse_hotkey(s: &str) -> Option<HotKey> {
    let parts: Vec<&str> = s.split('+').map(|p| p.trim()).collect();
    if parts.is_empty() {
        return None;
    }

    let mut modifiers = Modifiers::empty();
    let mut code = None;

    for part in parts {
        let lower = part.to_lowercase();
        match lower.as_str() {
            "ctrl" | "control" => modifiers |= Modifiers::CONTROL,
            "alt" => modifiers |= Modifiers::ALT,
            "shift" => modifiers |= Modifiers::SHIFT,
            "super" | "win" | "meta" | "cmd" => modifiers |= Modifiers::META,
            // Key codes
            "space" => code = Some(Code::Space),
            "enter" | "return" => code = Some(Code::Enter),
            "tab" => code = Some(Code::Tab),
            "esc" | "escape" => code = Some(Code::Escape),
            "backquote" | "`" => code = Some(Code::Backquote),
            "f1" => code = Some(Code::F1),
            "f2" => code = Some(Code::F2),
            "f3" => code = Some(Code::F3),
            "f4" => code = Some(Code::F4),
            "f5" => code = Some(Code::F5),
            "f6" => code = Some(Code::F6),
            "f7" => code = Some(Code::F7),
            "f8" => code = Some(Code::F8),
            "f9" => code = Some(Code::F9),
            "f10" => code = Some(Code::F10),
            "f11" => code = Some(Code::F11),
            "f12" => code = Some(Code::F12),
            single if single.len() == 1 => {
                let ch = single.chars().next()?.to_ascii_uppercase();
                code = match ch {
                    'A' => Some(Code::KeyA),
                    'B' => Some(Code::KeyB),
                    'C' => Some(Code::KeyC),
                    'D' => Some(Code::KeyD),
                    'E' => Some(Code::KeyE),
                    'F' => Some(Code::KeyF),
                    'G' => Some(Code::KeyG),
                    'H' => Some(Code::KeyH),
                    'I' => Some(Code::KeyI),
                    'J' => Some(Code::KeyJ),
                    'K' => Some(Code::KeyK),
                    'L' => Some(Code::KeyL),
                    'M' => Some(Code::KeyM),
                    'N' => Some(Code::KeyN),
                    'O' => Some(Code::KeyO),
                    'P' => Some(Code::KeyP),
                    'Q' => Some(Code::KeyQ),
                    'R' => Some(Code::KeyR),
                    'S' => Some(Code::KeyS),
                    'T' => Some(Code::KeyT),
                    'U' => Some(Code::KeyU),
                    'V' => Some(Code::KeyV),
                    'W' => Some(Code::KeyW),
                    'X' => Some(Code::KeyX),
                    'Y' => Some(Code::KeyY),
                    'Z' => Some(Code::KeyZ),
                    '0' => Some(Code::Digit0),
                    '1' => Some(Code::Digit1),
                    '2' => Some(Code::Digit2),
                    '3' => Some(Code::Digit3),
                    '4' => Some(Code::Digit4),
                    '5' => Some(Code::Digit5),
                    '6' => Some(Code::Digit6),
                    '7' => Some(Code::Digit7),
                    '8' => Some(Code::Digit8),
                    '9' => Some(Code::Digit9),
                    _ => None,
                };
            }
            _ => return None,
        }
    }

    let code = code?;
    let mods = if modifiers.is_empty() {
        None
    } else {
        Some(modifiers)
    };

    Some(HotKey::new(mods, code))
}

#[cfg(target_os = "linux")]
pub fn sync_gnome_hotkey(hotkey_str: &str) {
    let parts: Vec<&str> = hotkey_str.split('+').map(|p| p.trim()).collect();
    let mut gnome_mods = String::new();
    let mut key = String::new();

    for part in parts {
        let lower = part.to_lowercase();
        match lower.as_str() {
            "ctrl" | "control" => gnome_mods.push_str("<Control>"),
            "alt" => gnome_mods.push_str("<Alt>"),
            "shift" => gnome_mods.push_str("<Shift>"),
            "super" | "win" | "meta" | "cmd" => gnome_mods.push_str("<Super>"),
            "space" => key = "space".to_string(),
            "enter" | "return" => key = "Return".to_string(),
            "tab" => key = "Tab".to_string(),
            "esc" | "escape" => key = "Escape".to_string(),
            other => {
                if other.starts_with('f')
                    && other.len() > 1
                    && other[1..].chars().all(|c| c.is_ascii_digit())
                {
                    key = other.to_uppercase();
                } else if other.len() == 1 {
                    key = other.to_lowercase();
                }
            }
        }
    }

    if key.is_empty() {
        return;
    }

    let binding = format!("{}{}", gnome_mods, key);

    // If binding is <Alt>space, ensure GNOME's activate-window-menu does not intercept it
    if binding == "<Alt>space" {
        let _ = std::process::Command::new("gsettings")
            .args([
                "set",
                "org.gnome.desktop.wm.keybindings",
                "activate-window-menu",
                "[]",
            ])
            .status();
    }

    let path = "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/jlauncher/";
    let schema = format!(
        "org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:{}",
        path
    );

    let _ = std::process::Command::new("gsettings")
        .args(["set", &schema, "name", "JLauncher"])
        .status();
    let _ = std::process::Command::new("gsettings")
        .args(["set", &schema, "command", "jlauncher --toggle"])
        .status();
    let _ = std::process::Command::new("gsettings")
        .args(["set", &schema, "binding", &binding])
        .status();

    if let Ok(output) = std::process::Command::new("gsettings")
        .args([
            "get",
            "org.gnome.settings-daemon.plugins.media-keys",
            "custom-keybindings",
        ])
        .output()
    {
        let current = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !current.contains(path) && current.contains('[') {
            let new_list = current.replace(']', &format!(", '{}']", path));
            let _ = std::process::Command::new("gsettings")
                .args([
                    "set",
                    "org.gnome.settings-daemon.plugins.media-keys",
                    "custom-keybindings",
                    &new_list,
                ])
                .status();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_hotkeys() {
        assert!(parse_hotkey("Alt+Space").is_some());
        assert!(parse_hotkey("Ctrl+Space").is_some());
        assert!(parse_hotkey("Ctrl+Shift+P").is_some());
        assert!(parse_hotkey("Super+Space").is_some());
        assert!(parse_hotkey("F12").is_some());
    }

    #[test]
    fn test_parse_invalid_hotkeys() {
        assert!(parse_hotkey("").is_none());
        assert!(parse_hotkey("UnknownKeyName").is_none());
    }
}
