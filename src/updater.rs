use log::{error, info};
use serde::Deserialize;
use std::io::Read;
use std::sync::{Arc, Mutex};
use std::thread;

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const GITHUB_REPO: &str = "jools333/JLauncher";

#[derive(Debug, Clone, PartialEq)]
pub struct UpdateInfo {
    pub version: String,
    pub release_name: String,
    pub release_notes: String,
    pub download_url: String,
    pub asset_name: String,
    pub asset_size: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UpdateState {
    Idle,
    Checking,
    UpToDate,
    Available(UpdateInfo),
    Downloading {
        progress: f32,
        downloaded: u64,
        total: u64,
    },
    ReadyToRestart,
    Error(String),
}

#[derive(Debug, Deserialize)]
struct GithubReleaseAsset {
    name: String,
    size: u64,
    browser_download_url: String,
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    name: Option<String>,
    body: Option<String>,
    assets: Vec<GithubReleaseAsset>,
}

#[derive(Clone)]
pub struct UpdaterService {
    state: Arc<Mutex<UpdateState>>,
}

impl Default for UpdaterService {
    fn default() -> Self {
        Self::new()
    }
}

impl UpdaterService {
    pub fn new() -> Self {
        cleanup_old_executables();
        Self {
            state: Arc::new(Mutex::new(UpdateState::Idle)),
        }
    }

    pub fn state(&self) -> UpdateState {
        self.state
            .lock()
            .map(|s| s.clone())
            .unwrap_or(UpdateState::Idle)
    }

    pub fn check_in_background(&self) {
        let state_clone = Arc::clone(&self.state);
        if let Ok(mut state) = state_clone.lock() {
            if matches!(
                *state,
                UpdateState::Checking | UpdateState::Downloading { .. }
            ) {
                return;
            }
            *state = UpdateState::Checking;
        }

        thread::spawn(move || {
            info!("Updater: checking for updates on GitHub...");
            match check_latest_release() {
                Ok(Some(info)) => {
                    info!("Updater: found new version {}", info.version);
                    if let Ok(mut state) = state_clone.lock() {
                        *state = UpdateState::Available(info);
                    }
                }
                Ok(None) => {
                    info!("Updater: application is up-to-date (v{})", CURRENT_VERSION);
                    if let Ok(mut state) = state_clone.lock() {
                        *state = UpdateState::UpToDate;
                    }
                }
                Err(err) => {
                    error!("Updater check failed: {}", err);
                    if let Ok(mut state) = state_clone.lock() {
                        *state = UpdateState::Error(err);
                    }
                }
            }
        });
    }

    pub fn download_and_install_in_background(&self) {
        let info = match self.state() {
            UpdateState::Available(info) => info,
            _ => return,
        };

        let state_clone = Arc::clone(&self.state);
        if let Ok(mut state) = state_clone.lock() {
            *state = UpdateState::Downloading {
                progress: 0.0,
                downloaded: 0,
                total: info.asset_size,
            };
        }

        thread::spawn(move || {
            info!("Updater: starting download of {}", info.asset_name);
            let state_cb = Arc::clone(&state_clone);
            let res = download_and_install(&info, move |progress, downloaded, total| {
                if let Ok(mut s) = state_cb.lock() {
                    *s = UpdateState::Downloading {
                        progress,
                        downloaded,
                        total,
                    };
                }
            });

            if let Ok(mut state) = state_clone.lock() {
                match res {
                    Ok(_) => {
                        info!("Updater: download and install finished successfully");
                        *state = UpdateState::ReadyToRestart;
                    }
                    Err(e) => {
                        error!("Updater: download/install failed: {}", e);
                        *state = UpdateState::Error(e);
                    }
                }
            }
        });
    }

    pub fn restart() -> Result<(), String> {
        let current_exe = std::env::current_exe().map_err(|e| e.to_string())?;
        info!("Updater: restarting application: {:?}", current_exe);

        std::process::Command::new(&current_exe)
            .spawn()
            .map_err(|e| format!("Failed to spawn new process: {}", e))?;

        std::process::exit(0);
    }
}

fn check_latest_release() -> Result<Option<UpdateInfo>, String> {
    let url = format!(
        "https://api.github.com/repos/{}/releases/latest",
        GITHUB_REPO
    );

    let resp = ureq::get(&url)
        .set("User-Agent", &format!("JLauncher/{}", CURRENT_VERSION))
        .set("Accept", "application/vnd.github.v3+json")
        .call()
        .map_err(|e| format!("HTTP request error: {}", e))?;

    let release: GithubRelease = resp
        .into_json()
        .map_err(|e| format!("Failed to parse GitHub JSON response: {}", e))?;

    let remote_ver_str = release.tag_name.trim_start_matches('v').to_string();
    let is_newer = match (
        semver::Version::parse(&remote_ver_str),
        semver::Version::parse(CURRENT_VERSION),
    ) {
        (Ok(remote), Ok(current)) => remote > current,
        _ => remote_ver_str != CURRENT_VERSION,
    };

    if !is_newer {
        return Ok(None);
    }

    // Determine target asset for current OS
    let asset = find_platform_asset(&release.assets)
        .ok_or_else(|| "No compatible release asset found for current OS".to_string())?;

    Ok(Some(UpdateInfo {
        version: release.tag_name,
        release_name: release
            .name
            .unwrap_or_else(|| format!("JLauncher {}", remote_ver_str)),
        release_notes: release.body.unwrap_or_default(),
        download_url: asset.browser_download_url.clone(),
        asset_name: asset.name.clone(),
        asset_size: asset.size,
    }))
}

fn find_platform_asset(assets: &[GithubReleaseAsset]) -> Option<&GithubReleaseAsset> {
    #[cfg(target_os = "windows")]
    {
        // Prefer standalone exe with windows-x86_64
        if let Some(a) = assets
            .iter()
            .find(|a| a.name == "jlauncher-windows-x86_64.exe")
        {
            return Some(a);
        }
        if let Some(a) = assets.iter().find(|a| a.name.ends_with(".exe")) {
            return Some(a);
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        // Prefer standalone linux binary
        if let Some(a) = assets.iter().find(|a| a.name == "jlauncher-linux-x86_64") {
            return Some(a);
        }
        if let Some(a) = assets.iter().find(|a| a.name == "jlauncher") {
            return Some(a);
        }
    }

    None
}

fn download_and_install<F>(info: &UpdateInfo, on_progress: F) -> Result<(), String>
where
    F: Fn(f32, u64, u64),
{
    let current_exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let parent_dir = current_exe
        .parent()
        .ok_or_else(|| "Could not determine exe parent dir".to_string())?;

    let file_name = current_exe
        .file_name()
        .ok_or_else(|| "Could not determine exe file name".to_string())?
        .to_string_lossy();

    let temp_new_file = parent_dir.join(format!("{}.new_update", file_name));

    info!(
        "Downloading update from {} to {:?}",
        info.download_url, temp_new_file
    );

    let resp = ureq::get(&info.download_url)
        .set("User-Agent", &format!("JLauncher/{}", CURRENT_VERSION))
        .call()
        .map_err(|e| format!("Download request error: {}", e))?;

    let total_size = resp
        .header("content-length")
        .and_then(|l| l.parse::<u64>().ok())
        .unwrap_or(info.asset_size);

    let mut reader = resp.into_reader();
    let mut out_file = std::fs::File::create(&temp_new_file)
        .map_err(|e| format!("Could not create temp file {:?}: {}", temp_new_file, e))?;

    let mut downloaded = 0u64;
    let mut buffer = [0u8; 32768]; // 32KB buffer

    loop {
        let n = reader
            .read(&mut buffer)
            .map_err(|e| format!("Error while reading download stream: {}", e))?;

        if n == 0 {
            break;
        }

        std::io::Write::write_all(&mut out_file, &buffer[..n])
            .map_err(|e| format!("Failed writing to temp file: {}", e))?;

        downloaded += n as u64;
        let progress = if total_size > 0 {
            (downloaded as f32 / total_size as f32).min(1.0)
        } else {
            0.0
        };

        on_progress(progress, downloaded, total_size);
    }

    drop(out_file);
    on_progress(1.0, downloaded, total_size);

    // Apply permissions on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(&temp_new_file) {
            let mut perms = meta.permissions();
            perms.set_mode(0o755);
            let _ = std::fs::set_permissions(&temp_new_file, perms);
        }
    }

    // Replace the running binary
    #[cfg(target_os = "windows")]
    {
        let old_exe = current_exe.with_extension("exe.old");
        let _ = std::fs::remove_file(&old_exe);

        std::fs::rename(&current_exe, &old_exe)
            .map_err(|e| format!("Failed renaming current exe to {:?}: {}", old_exe, e))?;

        if let Err(e) = std::fs::rename(&temp_new_file, &current_exe) {
            // Attempt rollback if replacement fails
            let _ = std::fs::rename(&old_exe, &current_exe);
            return Err(format!("Failed moving new exe into place: {}", e));
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        std::fs::rename(&temp_new_file, &current_exe)
            .map_err(|e| format!("Failed replacing executable {:?}: {}", current_exe, e))?;
    }

    Ok(())
}

pub fn cleanup_old_executables() {
    #[cfg(target_os = "windows")]
    if let Ok(current_exe) = std::env::current_exe() {
        let old_exe = current_exe.with_extension("exe.old");
        if old_exe.exists() {
            let _ = std::fs::remove_file(old_exe);
        }
    }
}
