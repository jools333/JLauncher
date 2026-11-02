#![allow(dead_code)]

use super::AppEntry;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn scan_linux_apps() -> Vec<AppEntry> {
    let mut apps = Vec::new();
    let mut search_dirs = vec![
        PathBuf::from("/usr/share/applications"),
        PathBuf::from("/usr/local/share/applications"),
        PathBuf::from("/var/lib/flatpak/exports/share/applications"),
        PathBuf::from("/var/lib/snapd/desktop/applications"),
    ];

    if let Ok(home) = env::var("HOME") {
        let home_path = PathBuf::from(home);
        search_dirs.push(home_path.join(".local/share/applications"));
        search_dirs.push(home_path.join(".local/share/flatpak/exports/share/applications"));
    }

    for dir in search_dirs {
        if dir.exists() {
            scan_desktop_dir(&dir, &mut apps);
        }
    }

    apps
}

fn scan_desktop_dir(dir: &Path, apps: &mut Vec<AppEntry>) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    let is_ru = env::var("LANG").map_or(false, |l| l.starts_with("ru"));

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && path.extension().map_or(false, |e| e == "desktop") {
            if let Some(app) = parse_desktop_file(&path, is_ru) {
                apps.push(app);
            }
        }
    }
}

fn parse_desktop_file(path: &Path, prefer_russian: bool) -> Option<AppEntry> {
    let content = fs::read_to_string(path).ok()?;

    let mut in_desktop_entry = false;
    let mut is_app = false;
    let mut no_display = false;
    let mut name = None;
    let mut name_ru = None;
    let mut comment = None;
    let mut comment_ru = None;
    let mut exec = None;
    let mut icon = None;
    let mut keywords = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            if line == "[Desktop Entry]" {
                in_desktop_entry = true;
            } else if in_desktop_entry {
                // Entered another group
                break;
            }
            continue;
        }

        if !in_desktop_entry {
            continue;
        }

        if let Some((k, v)) = line.split_once('=') {
            let k = k.trim();
            let v = v.trim();

            match k {
                "Type" => {
                    if v == "Application" {
                        is_app = true;
                    }
                }
                "NoDisplay" => {
                    if v.eq_ignore_ascii_case("true") {
                        no_display = true;
                    }
                }
                "Name" => {
                    if name.is_none() {
                        name = Some(v.to_string());
                    }
                }
                "Name[ru]" => {
                    name_ru = Some(v.to_string());
                }
                "Comment" => {
                    if comment.is_none() {
                        comment = Some(v.to_string());
                    }
                }
                "Comment[ru]" => {
                    comment_ru = Some(v.to_string());
                }
                "Exec" => {
                    if exec.is_none() {
                        exec = Some(clean_exec_command(v));
                    }
                }
                "Icon" => {
                    if icon.is_none() {
                        icon = Some(v.to_string());
                    }
                }
                "Keywords" | "Keywords[ru]" => {
                    for kw in v.split(';') {
                        let trimmed = kw.trim();
                        if !trimmed.is_empty() {
                            keywords.push(trimmed.to_lowercase());
                        }
                    }
                }
                _ => {}
            }
        }
    }

    if !is_app || no_display {
        return None;
    }

    let display_name = if prefer_russian {
        name_ru.or(name)?
    } else {
        name.or(name_ru)?
    };

    let display_comment = if prefer_russian {
        comment_ru.or(comment).unwrap_or_default()
    } else {
        comment.or(comment_ru).unwrap_or_default()
    };

    let exec_cmd = exec?;
    let file_id = path.file_name()?.to_string_lossy().to_string();

    Some(AppEntry {
        id: file_id,
        name: display_name,
        description: display_comment,
        exec: exec_cmd,
        icon,
        keywords,
    })
}

fn clean_exec_command(exec: &str) -> String {
    // Desktop files use %f, %F, %u, %U, %d, %D, %n, %N, %i, %c, %k, %v, %m
    let parts: Vec<&str> = exec
        .split_whitespace()
        .filter(|part| !part.starts_with('%'))
        .collect();
    parts.join(" ")
}

pub fn launch_app(entry: &AppEntry) -> Result<(), String> {
    // If we have a desktop ID, try gtk-launch or gio launch first
    if entry.id.ends_with(".desktop") {
        if let Ok(mut child) = Command::new("gtk-launch").arg(&entry.id).spawn() {
            let _ = child.wait();
            return Ok(());
        }
    }

    // Fallback: spawn command via sh -c detached
    let shell_cmd = format!("{} &", entry.exec);
    Command::new("sh")
        .args(["-c", &shell_cmd])
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}
