#![allow(dead_code)]

use super::AppEntry;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn scan_windows_apps() -> Vec<AppEntry> {
    let mut apps = Vec::new();

    // 1. Built-in common Windows utilities
    apps.extend(builtin_windows_apps());

    // 2. Start menu locations
    let mut search_dirs = Vec::new();

    if let Ok(progdata) = env::var("ProgramData") {
        let p = PathBuf::from(progdata).join("Microsoft").join("Windows").join("Start Menu").join("Programs");
        if p.exists() {
            search_dirs.push(p);
        }
    }

    if let Ok(appdata) = env::var("APPDATA") {
        let p = PathBuf::from(appdata).join("Microsoft").join("Windows").join("Start Menu").join("Programs");
        if p.exists() {
            search_dirs.push(p);
        }
    }

    for dir in search_dirs {
        scan_dir_recursive(&dir, &mut apps);
    }

    apps
}

fn builtin_windows_apps() -> Vec<AppEntry> {
    vec![
        AppEntry {
            id: "calc".into(),
            name: "Calculator / Калькулятор".into(),
            description: "Windows Calculator".into(),
            exec: "calc.exe".into(),
            icon: Some("🧮".into()),
            keywords: vec!["calculator".into(), "калькулятор".into(), "calc".into()],
        },
        AppEntry {
            id: "notepad".into(),
            name: "Notepad / Блокнот".into(),
            description: "Text Editor".into(),
            exec: "notepad.exe".into(),
            icon: Some("📝".into()),
            keywords: vec!["notepad".into(), "блокнот".into(), "текст".into(), "editor".into()],
        },
        AppEntry {
            id: "cmd".into(),
            name: "Command Prompt / Командная строка".into(),
            description: "Windows Command Processor".into(),
            exec: "cmd.exe".into(),
            icon: Some("💻".into()),
            keywords: vec!["cmd".into(), "терминал".into(), "командная строка".into(), "terminal".into()],
        },
        AppEntry {
            id: "powershell".into(),
            name: "PowerShell".into(),
            description: "Windows PowerShell".into(),
            exec: "powershell.exe".into(),
            icon: Some("💻".into()),
            keywords: vec!["powershell".into(), "терминал".into(), "ps".into()],
        },
        AppEntry {
            id: "taskmgr".into(),
            name: "Task Manager / Диспетчер задач".into(),
            description: "View and manage running processes".into(),
            exec: "taskmgr.exe".into(),
            icon: Some("📊".into()),
            keywords: vec!["taskmgr".into(), "диспетчер".into(), "процессы".into(), "tasks".into()],
        },
        AppEntry {
            id: "explorer".into(),
            name: "File Explorer / Проводник".into(),
            description: "Browse files and folders".into(),
            exec: "explorer.exe".into(),
            icon: Some("📁".into()),
            keywords: vec!["explorer".into(), "проводник".into(), "файлы".into(), "папки".into()],
        },
        AppEntry {
            id: "settings".into(),
            name: "Settings / Параметры".into(),
            description: "Windows Settings".into(),
            exec: "ms-settings:".into(),
            icon: Some("⚙".into()),
            keywords: vec!["settings".into(), "параметры".into(), "настройки".into(), "конфиг".into()],
        },
        AppEntry {
            id: "control".into(),
            name: "Control Panel / Панель управления".into(),
            description: "Windows Control Panel".into(),
            exec: "control.exe".into(),
            icon: Some("🛠".into()),
            keywords: vec!["control".into(), "панель управления".into(), "control panel".into()],
        },
    ]
}

fn scan_dir_recursive(dir: &Path, apps: &mut Vec<AppEntry>) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan_dir_recursive(&path, apps);
        } else if let Some(ext) = path.extension() {
            let ext_str = ext.to_string_lossy().to_lowercase();
            if ext_str == "lnk" || ext_str == "url" {
                if let Some(stem) = path.file_stem() {
                    let name = stem.to_string_lossy().to_string();

                    // Filter out uninstaller and helper shortcuts
                    let lower = name.to_lowercase();
                    if lower.contains("uninstall")
                        || lower.contains("деинсталл")
                        || lower.contains("read me")
                        || lower.contains("readme")
                        || lower.contains("help")
                        || lower.contains("помощь")
                    {
                        continue;
                    }

                    let exec_str = path.to_string_lossy().to_string();
                    apps.push(AppEntry {
                        id: exec_str.clone(),
                        name: name.clone(),
                        description: "Installed Application".into(),
                        exec: exec_str,
                        icon: Some("🚀".into()),
                        keywords: vec![name.to_lowercase()],
                    });
                }
            }
        }
    }
}

pub fn launch_app(entry: &AppEntry) -> Result<(), String> {
    if entry.exec.starts_with("ms-settings:") {
        open::that_detached(&entry.exec).map_err(|e| e.to_string())?;
        return Ok(());
    }

    if entry.exec.ends_with(".lnk") || entry.exec.ends_with(".url") {
        Command::new("cmd")
            .args(["/c", "start", "", &entry.exec])
            .spawn()
            .map_err(|e| e.to_string())?;
        return Ok(());
    }

    // Try spawning directly or fallback to open::that_detached
    if Command::new(&entry.exec).spawn().is_ok() {
        return Ok(());
    }

    open::that_detached(&entry.exec).map_err(|e| e.to_string())
}
