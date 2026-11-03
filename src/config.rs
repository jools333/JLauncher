use auto_launch::AutoLaunchBuilder;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub hotkey: String,
    pub autostart: bool,
    pub max_results: usize,
    pub clear_on_hide: bool,
    pub dark_theme: bool,
    pub enable_calculator: bool,
    pub enable_system_commands: bool,
    #[serde(default = "default_true")]
    pub auto_check_updates: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            hotkey: "Alt+Space".to_string(),
            autostart: false,
            max_results: 8,
            clear_on_hide: true,
            dark_theme: true,
            enable_calculator: true,
            enable_system_commands: true,
            auto_check_updates: true,
        }
    }
}

impl Config {
    pub fn config_path() -> Option<PathBuf> {
        let proj_dirs = ProjectDirs::from("org", "jlauncher", "jlauncher")?;
        let dir = proj_dirs.config_dir();
        fs::create_dir_all(dir).ok()?;
        Some(dir.join("config.json"))
    }

    pub fn load() -> Self {
        if let Some(path) = Self::config_path() {
            if path.exists() {
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok(config) = serde_json::from_str::<Config>(&content) {
                        return config;
                    }
                }
            }
        }
        Self::default()
    }

    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(path) = Self::config_path() {
            let json = serde_json::to_string_pretty(self)?;
            fs::write(path, json)?;
        }
        Ok(())
    }

    pub fn sync_autostart(&self) -> Result<(), Box<dyn std::error::Error>> {
        let exe_path = std::env::current_exe()?;
        let auto = AutoLaunchBuilder::new()
            .set_app_name("JLauncher")
            .set_app_path(exe_path.to_str().unwrap_or("jlauncher"))
            .set_args(&["--hidden"])
            .build()?;

        if self.autostart {
            let _ = auto.enable();
        } else {
            let _ = auto.disable();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default_and_serde() {
        let default_config = Config::default();
        assert_eq!(default_config.hotkey, "Alt+Space");
        assert_eq!(default_config.max_results, 8);
        assert!(default_config.enable_calculator);
        assert!(default_config.enable_system_commands);
        assert!(default_config.auto_check_updates);

        let json = serde_json::to_string(&default_config).unwrap();
        let loaded: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.hotkey, "Alt+Space");
    }
}
