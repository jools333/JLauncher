use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::Instant;
use log::info;

pub struct IconIndex {
    map: Arc<RwLock<HashMap<String, PathBuf>>>,
}

impl IconIndex {
    pub fn new() -> Self {
        let map = Arc::new(RwLock::new(HashMap::new()));
        let map_clone = Arc::clone(&map);

        // Spawn background thread to index system PNG icons without blocking startup
        std::thread::Builder::new()
            .name("jlauncher-icon-indexer".into())
            .spawn(move || {
                let start = Instant::now();
                let indexed = index_system_icons();
                let count = indexed.len();
                {
                    let mut lock = map_clone.write().unwrap();
                    *lock = indexed;
                }
                info!("IconIndex: indexed {} PNG icons in {:?}", count, start.elapsed());
            })
            .ok();

        Self { map }
    }

    pub fn find_icon_path(&self, icon_str: &str) -> Option<PathBuf> {
        let trimmed = icon_str.trim();
        if trimmed.is_empty() {
            return None;
        }

        // 1. If it looks like a direct path
        if trimmed.starts_with('/') || trimmed.contains('\\') {
            let path = PathBuf::from(trimmed);
            if path.is_file() {
                if path.extension().map_or(false, |ext| ext.eq_ignore_ascii_case("png")) {
                    return Some(path);
                }
                // If it's another extension, try .png
                let png_path = path.with_extension("png");
                if png_path.is_file() {
                    return Some(png_path);
                }
            }

            // Also try looking up the file stem in our index
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                let stem_lower = stem.to_lowercase();
                if let Ok(lock) = self.map.read() {
                    if let Some(p) = lock.get(&stem_lower) {
                        return Some(p.clone());
                    }
                }
            }
        }

        let key = trimmed.to_lowercase();
        let lock = self.map.read().ok()?;

        // 2. Direct name match in index
        if let Some(p) = lock.get(&key) {
            return Some(p.clone());
        }

        // 3. Strip -symbolic or -panel or .desktop
        let stripped = key
            .strip_suffix("-symbolic")
            .or_else(|| key.strip_suffix(".desktop"))
            .unwrap_or(&key);

        if let Some(p) = lock.get(stripped) {
            return Some(p.clone());
        }

        // 4. Try last component if separated by dots (e.g. org.gnome.Calculator -> calculator)
        if let Some(last_dot) = stripped.rfind('.') {
            let sub = &stripped[last_dot + 1..];
            if let Some(p) = lock.get(sub) {
                return Some(p.clone());
            }
            // Also try kebab-case of last two (e.g. org.gnome.DejaDup -> deja-dup)
            let without_prefix = stripped.strip_prefix("org.gnome.").unwrap_or(stripped);
            let kebab = camel_to_kebab(without_prefix);
            if let Some(p) = lock.get(&kebab) {
                return Some(p.clone());
            }
        }

        None
    }
}

fn camel_to_kebab(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            result.push('-');
        }
        result.push(c.to_ascii_lowercase());
    }
    result
}

fn index_system_icons() -> HashMap<String, PathBuf> {
    #[cfg(target_os = "windows")]
    {
        // On Windows, system apps and desktop shortcuts
        return HashMap::new();
    }

    #[cfg(not(target_os = "windows"))]
    {
        let mut map = HashMap::new();
        let mut search_dirs = vec![
            PathBuf::from("/usr/share/pixmaps"),
            PathBuf::from("/usr/share/icons/hicolor"),
            PathBuf::from("/usr/share/icons/Yaru"),
            PathBuf::from("/usr/share/icons"),
            PathBuf::from("/var/lib/flatpak/exports/share/icons"),
            PathBuf::from("/snap/gtk-common-themes/current/share/icons"),
        ];

        if let Ok(home) = std::env::var("HOME") {
            let home_path = PathBuf::from(home);
            search_dirs.push(home_path.join(".local/share/icons"));
            search_dirs.push(home_path.join(".icons"));
        }

        for dir in search_dirs {
            if dir.is_dir() {
                scan_dir_for_pngs(&dir, &mut map);
            }
        }

        map
    }
}

#[cfg(not(target_os = "windows"))]
fn scan_dir_for_pngs(dir: &Path, map: &mut HashMap<String, PathBuf>) {
    let mut stack = vec![dir.to_path_buf()];

    while let Some(current) = stack.pop() {
        let entries = match fs::read_dir(&current) {
            Ok(e) => e,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // Skip very deep or irrelevant dirs
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if name == "cursors" || name == "16x16" || name == "22x22" || name == "24x24" {
                    // Skip tiny icons when scanning icons root if larger exist
                    continue;
                }
                stack.push(path);
            } else if path.is_file() {
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if ext.eq_ignore_ascii_case("png") {
                        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                            let key = stem.to_lowercase();
                            let path_str = path.to_string_lossy();
                            let is_preferred_size = path_str.contains("64x64")
                                || path_str.contains("48x48")
                                || path_str.contains("128x128")
                                || path_str.contains("256x256");

                            if !map.contains_key(&key) || is_preferred_size {
                                map.insert(key, path);
                            }
                        }
                    }
                }
            }
        }
    }
}

pub fn load_icon_texture(
    ctx: &eframe::egui::Context,
    name: &str,
    path: &Path,
) -> Option<eframe::egui::TextureHandle> {
    let img_bytes = fs::read(path).ok()?;
    let dynamic_img = image::load_from_memory(&img_bytes).ok()?;

    // Resize down to max 72x72 for sharp rendering and minimal memory footprint
    let resized = if dynamic_img.width() > 72 || dynamic_img.height() > 72 {
        dynamic_img.resize(72, 72, image::imageops::FilterType::Triangle)
    } else {
        dynamic_img
    };

    let rgba = resized.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    let pixels = rgba.into_raw();
    let color_image = eframe::egui::ColorImage::from_rgba_unmultiplied(size, &pixels);

    Some(ctx.load_texture(
        format!("icon_{}", name),
        color_image,
        eframe::egui::TextureOptions::LINEAR,
    ))
}
