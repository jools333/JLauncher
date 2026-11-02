use std::process::Command;

#[derive(Debug, Clone)]
pub struct SystemAction {
    pub id: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub icon: &'static str,
    pub keywords: &'static [&'static str],
}

pub const SYSTEM_ACTIONS: &[SystemAction] = &[
    SystemAction {
        id: "shutdown",
        title: "Shutdown / Выключение",
        description: "Power off the computer",
        icon: "⏻",
        keywords: &["shutdown", "poweroff", "power off", "turn off", "выключить", "выключение", "питание", "off"],
    },
    SystemAction {
        id: "reboot",
        title: "Restart / Перезагрузка",
        description: "Restart the computer",
        icon: "↻",
        keywords: &["reboot", "restart", "перезагрузка", "перезагрузить", "рестарт"],
    },
    SystemAction {
        id: "lock",
        title: "Lock Screen / Заблокировать экран",
        description: "Lock the current session",
        icon: "🔒",
        keywords: &["lock", "lock screen", "заблокировать", "блокировка", "экран"],
    },
    SystemAction {
        id: "logout",
        title: "Log Out / Завершить сеанс",
        description: "Sign out of the current user session",
        icon: "🚪",
        keywords: &["logout", "log out", "sign out", "выйти", "завершить сеанс", "сеанс"],
    },
    SystemAction {
        id: "sleep",
        title: "Sleep / Спящий режим",
        description: "Put the computer into suspend mode",
        icon: "🌙",
        keywords: &["sleep", "suspend", "сон", "спящий режим", "приостановить"],
    },
];

pub fn execute_action(id: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        match id {
            "shutdown" => {
                Command::new("shutdown")
                    .args(["/s", "/t", "0"])
                    .spawn()
                    .map_err(|e| e.to_string())?;
            }
            "reboot" => {
                Command::new("shutdown")
                    .args(["/r", "/t", "0"])
                    .spawn()
                    .map_err(|e| e.to_string())?;
            }
            "lock" => {
                Command::new("rundll32.exe")
                    .args(["user32.dll,LockWorkStation"])
                    .spawn()
                    .map_err(|e| e.to_string())?;
            }
            "logout" => {
                Command::new("shutdown")
                    .args(["/l"])
                    .spawn()
                    .map_err(|e| e.to_string())?;
            }
            "sleep" => {
                Command::new("rundll32.exe")
                    .args(["powrprof.dll,SetSuspendState", "0,1,0"])
                    .spawn()
                    .map_err(|e| e.to_string())?;
            }
            _ => return Err("Unknown action".to_string()),
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        match id {
            "shutdown" => {
                let status = Command::new("systemctl")
                    .arg("poweroff")
                    .spawn()
                    .or_else(|_| Command::new("shutdown").args(["-h", "now"]).spawn());
                status.map_err(|e| e.to_string())?;
            }
            "reboot" => {
                let status = Command::new("systemctl")
                    .arg("reboot")
                    .spawn()
                    .or_else(|_| Command::new("reboot").spawn());
                status.map_err(|e| e.to_string())?;
            }
            "lock" => {
                let status = Command::new("loginctl")
                    .arg("lock-session")
                    .spawn()
                    .or_else(|_| Command::new("xdg-screensaver").arg("lock").spawn());
                status.map_err(|e| e.to_string())?;
            }
            "logout" => {
                let user = std::env::var("USER").unwrap_or_default();
                let status = Command::new("loginctl")
                    .args(["terminate-user", &user])
                    .spawn()
                    .or_else(|_| Command::new("pkill").args(["-KILL", "-u", &user]).spawn());
                status.map_err(|e| e.to_string())?;
            }
            "sleep" => {
                let status = Command::new("systemctl")
                    .arg("suspend")
                    .spawn();
                status.map_err(|e| e.to_string())?;
            }
            _ => return Err("Unknown action".to_string()),
        }
    }

    Ok(())
}
