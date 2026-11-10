use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::AppEntry;

/// Tracks application launch counts and persists them to disk.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppUsage {
    pub counts: HashMap<String, u64>,
    #[serde(skip)]
    custom_path: Option<PathBuf>,
}

impl AppUsage {
    pub fn new() -> Self {
        Self::load()
    }

    #[allow(dead_code)]
    pub fn with_path(path: PathBuf) -> Self {
        let mut usage = Self::load_from_path(&path).unwrap_or_default();
        usage.custom_path = Some(path);
        usage
    }

    pub fn default_path() -> Option<PathBuf> {
        let proj_dirs = ProjectDirs::from("org", "jlauncher", "jlauncher")?;
        let dir = proj_dirs.config_dir();
        fs::create_dir_all(dir).ok()?;
        Some(dir.join("usage.json"))
    }

    pub fn load() -> Self {
        if let Some(path) = Self::default_path() {
            if let Ok(usage) = Self::load_from_path(&path) {
                return usage;
            }
        }
        Self::default()
    }

    pub fn load_from_path(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        if path.exists() {
            let content = fs::read_to_string(path)?;
            let mut usage: AppUsage = serde_json::from_str(&content)?;
            usage.custom_path = Some(path.to_path_buf());
            return Ok(usage);
        }
        Ok(Self {
            custom_path: Some(path.to_path_buf()),
            ..Default::default()
        })
    }

    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let path = self.custom_path.clone().or_else(Self::default_path);
        if let Some(path) = path {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            let json = serde_json::to_string_pretty(self)?;
            fs::write(path, json)?;
        }
        Ok(())
    }

    pub fn get_count(&self, key: &str) -> u64 {
        self.counts.get(key).copied().unwrap_or(0)
    }

    pub fn get_count_for_app(&self, app: &AppEntry) -> u64 {
        if !app.id.is_empty() {
            let count = self.get_count(&app.id);
            if count > 0 {
                return count;
            }
        }
        if !app.name.is_empty() {
            let count = self.get_count(&app.name);
            if count > 0 {
                return count;
            }
        }
        if !app.exec.is_empty() {
            let count = self.get_count(&app.exec);
            if count > 0 {
                return count;
            }
        }
        0
    }

    pub fn record_launch(&mut self, app: &AppEntry) -> u64 {
        let key = if !app.id.is_empty() {
            &app.id
        } else if !app.name.is_empty() {
            &app.name
        } else {
            &app.exec
        };
        let count = self.counts.entry(key.to_string()).or_insert(0);
        *count = count.saturating_add(1);
        let updated = *count;
        if let Err(e) = self.save() {
            log::warn!("Failed to save usage stats: {}", e);
        }
        updated
    }

    pub fn clear(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.counts.clear();
        self.save()
    }
}

/// Calculates the search score bonus for an application based on its launch count.
/// Uses a smooth logarithmic scale so frequent apps get a noticeable boost,
/// while preventing an accidental partial match from overwhelming an exact match.
pub fn calculate_usage_boost(count: u64) -> i64 {
    if count == 0 {
        return 0;
    }
    ((count as f64).ln_1p() * 15.0).round() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_and_get_count() {
        let dir = std::env::temp_dir().join(format!("jlauncher_test_usage_{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let file_path = dir.join("usage.json");
        let _ = fs::remove_file(&file_path);

        let mut usage = AppUsage::with_path(file_path.clone());
        let app = AppEntry {
            id: "org.test.App.desktop".into(),
            name: "Test App".into(),
            description: "A test app".into(),
            exec: "/usr/bin/test-app".into(),
            icon: None,
            keywords: vec![],
        };

        assert_eq!(usage.get_count_for_app(&app), 0);
        assert_eq!(usage.record_launch(&app), 1);
        assert_eq!(usage.get_count_for_app(&app), 1);
        assert_eq!(usage.record_launch(&app), 2);
        assert_eq!(usage.get_count_for_app(&app), 2);

        // Verify persistence by loading from the same path
        let loaded = AppUsage::load_from_path(&file_path).unwrap();
        assert_eq!(loaded.get_count_for_app(&app), 2);

        let _ = fs::remove_file(file_path);
        let _ = fs::remove_dir(dir);
    }

    #[test]
    fn test_usage_boost_curve() {
        assert_eq!(calculate_usage_boost(0), 0);
        let b1 = calculate_usage_boost(1);
        let b5 = calculate_usage_boost(5);
        let b20 = calculate_usage_boost(20);
        let b100 = calculate_usage_boost(100);

        assert!(b1 > 0, "1 launch should give positive boost");
        assert!(b5 > b1, "5 launches should give higher boost than 1");
        assert!(b20 > b5, "20 launches should give higher boost than 5");
        assert!(b100 > b20, "100 launches should give higher boost than 20");
    }
}
