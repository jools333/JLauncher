use eframe::egui;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

static IS_VISIBLE: AtomicBool = AtomicBool::new(true);
static SHOW_SETTINGS: AtomicBool = AtomicBool::new(false);
static SHOULD_RESCAN: AtomicBool = AtomicBool::new(false);
static SHOULD_QUIT: AtomicBool = AtomicBool::new(false);
#[allow(dead_code)]
static MAIN_HWND: AtomicIsize = AtomicIsize::new(0);
static REVISION: AtomicU64 = AtomicU64::new(1);
static EGUI_CTX: Mutex<Option<egui::Context>> = Mutex::new(None);
static LAST_TOGGLE: Mutex<Option<Instant>> = Mutex::new(None);

pub fn init(ctx: egui::Context, start_hidden: bool) {
    IS_VISIBLE.store(!start_hidden, Ordering::SeqCst);
    SHOW_SETTINGS.store(false, Ordering::SeqCst);
    SHOULD_RESCAN.store(false, Ordering::SeqCst);
    SHOULD_QUIT.store(false, Ordering::SeqCst);
    REVISION.store(1, Ordering::SeqCst);

    let mut lock = EGUI_CTX.lock().unwrap();
    *lock = Some(ctx);
}

#[allow(dead_code)]
pub fn set_main_hwnd(hwnd: isize) {
    if hwnd != 0 {
        let old = MAIN_HWND.swap(hwnd, Ordering::SeqCst);
        if old != hwnd {
            log::info!("Coordinator: main window HWND registered as 0x{:x}", hwnd);
            #[cfg(target_os = "windows")]
            apply_rounded_corners(hwnd);
        }
    }
}

#[allow(dead_code)]
pub fn get_main_hwnd() -> isize {
    MAIN_HWND.load(Ordering::SeqCst)
}

#[cfg(target_os = "windows")]
pub fn apply_rounded_corners(hwnd: isize) {
    if hwnd == 0 {
        return;
    }
    unsafe {
        use windows_sys::Win32::Graphics::Gdi::{CreateRoundRectRgn, SetWindowRgn};
        use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowRect;

        let h = hwnd as windows_sys::Win32::Foundation::HWND;
        let mut rect = std::mem::zeroed();
        if GetWindowRect(h, &mut rect) != 0 {
            let width = rect.right - rect.left;
            let height = rect.bottom - rect.top;
            if width > 0 && height > 0 {
                let scale = (width as f32 / 650.0).max(1.0);
                let radius = (12.0 * scale).round() as i32;
                let diameter = radius * 2;
                let hrgn = CreateRoundRectRgn(0, 0, width + 1, height + 1, diameter, diameter);
                if !hrgn.is_null() {
                    SetWindowRgn(h, hrgn, 1);
                }
            }
        }

        // Try Windows 11 DWM rounded corners if available
        use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryA};
        let dwmapi = LoadLibraryA(b"dwmapi.dll\0".as_ptr());
        if !dwmapi.is_null() {
            let func = GetProcAddress(dwmapi, b"DwmSetWindowAttribute\0".as_ptr());
            if let Some(dwm_set_attr) = func {
                type DwmSetAttrFn = unsafe extern "system" fn(
                    windows_sys::Win32::Foundation::HWND,
                    u32,
                    *const std::ffi::c_void,
                    u32,
                ) -> i32;
                let dwm_set_attr: DwmSetAttrFn = std::mem::transmute(dwm_set_attr);
                const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
                const DWMWCP_ROUND: u32 = 2;
                let pref: u32 = DWMWCP_ROUND;
                let _ = dwm_set_attr(
                    h,
                    DWMWA_WINDOW_CORNER_PREFERENCE,
                    &pref as *const u32 as *const std::ffi::c_void,
                    std::mem::size_of::<u32>() as u32,
                );
            }
        }
    }
}

#[cfg(target_os = "windows")]
pub fn ensure_main_hwnd() -> isize {
    let existing = MAIN_HWND.load(Ordering::SeqCst);
    if existing != 0 {
        return existing;
    }

    use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM};
    use windows_sys::Win32::System::Threading::GetCurrentProcessId;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetClassNameW, GetWindowTextW, GetWindowThreadProcessId,
    };

    unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == GetCurrentProcessId() {
            let mut title_buf = [0u16; 128];
            let title_len = GetWindowTextW(hwnd, title_buf.as_mut_ptr(), 128);
            let title_str = if title_len > 0 {
                String::from_utf16_lossy(&title_buf[..title_len as usize])
            } else {
                String::new()
            };

            let mut class_buf = [0u16; 128];
            let class_len = GetClassNameW(hwnd, class_buf.as_mut_ptr(), 128);
            let class_str = if class_len > 0 {
                String::from_utf16_lossy(&class_buf[..class_len as usize])
            } else {
                String::new()
            };

            if title_str == "JLauncher" && !class_str.contains("Tray") {
                let result_ptr = lparam as *mut isize;
                *result_ptr = hwnd as isize;
                return 0;
            }
        }
        1
    }

    let mut result: isize = 0;
    unsafe {
        EnumWindows(Some(enum_proc), &mut result as *mut isize as LPARAM);
    }
    if result != 0 {
        set_main_hwnd(result);
    }
    result
}

pub fn is_visible() -> bool {
    IS_VISIBLE.load(Ordering::SeqCst)
}

pub fn show_settings() -> bool {
    SHOW_SETTINGS.load(Ordering::SeqCst)
}

pub fn set_show_settings(val: bool) {
    SHOW_SETTINGS.store(val, Ordering::SeqCst);
}

pub fn check_and_clear_rescan() -> bool {
    SHOULD_RESCAN.swap(false, Ordering::SeqCst)
}

pub fn should_quit() -> bool {
    SHOULD_QUIT.load(Ordering::SeqCst)
}

pub fn get_revision() -> u64 {
    REVISION.load(Ordering::SeqCst)
}

pub fn get_screen_size() -> (f32, f32) {
    #[cfg(not(target_os = "windows"))]
    {
        if let Ok((conn, screen_num)) = x11rb::connect(None) {
            use x11rb::connection::Connection;
            let setup = conn.setup();
            if let Some(screen) = setup.roots.get(screen_num) {
                let w = screen.width_in_pixels as f32;
                let h = screen.height_in_pixels as f32;
                if w > 0.0 && h > 0.0 {
                    return (w, h);
                }
            }
        }
        (1920.0, 1080.0)
    }
    #[cfg(target_os = "windows")]
    {
        unsafe {
            use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};
            let w = GetSystemMetrics(SM_CXSCREEN) as f32;
            let h = GetSystemMetrics(SM_CYSCREEN) as f32;
            if w > 0.0 && h > 0.0 {
                (w, h)
            } else {
                (1920.0, 1080.0)
            }
        }
    }
}

pub fn show_window(with_settings: bool) {
    log::info!("Coordinator: show_window(with_settings={})", with_settings);
    IS_VISIBLE.store(true, Ordering::SeqCst);
    SHOW_SETTINGS.store(with_settings, Ordering::SeqCst);
    REVISION.fetch_add(1, Ordering::SeqCst);

    #[cfg(target_os = "windows")]
    {
        let hwnd = ensure_main_hwnd();
        if hwnd != 0 {
            unsafe {
                use windows_sys::Win32::System::Threading::{
                    AttachThreadInput, GetCurrentProcessId, GetCurrentThreadId,
                };
                use windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus;
                use windows_sys::Win32::UI::WindowsAndMessaging::{
                    AllowSetForegroundWindow, BringWindowToTop, GetForegroundWindow,
                    GetWindowThreadProcessId, SetForegroundWindow, SetWindowPos,
                    ShowWindow, HWND_TOPMOST, SWP_SHOWWINDOW, SW_RESTORE, SW_SHOW,
                };
                let h = hwnd as windows_sys::Win32::Foundation::HWND;
                AllowSetForegroundWindow(GetCurrentProcessId());

                let (screen_w, screen_h) = get_screen_size();

                ShowWindow(h, SW_RESTORE);
                ShowWindow(h, SW_SHOW);
                SetWindowPos(h, HWND_TOPMOST, 0, 0, screen_w as i32, screen_h as i32, SWP_SHOWWINDOW);

                let fg_hwnd = GetForegroundWindow();
                let current_thread = GetCurrentThreadId();
                let target_thread = GetWindowThreadProcessId(h, std::ptr::null_mut());
                let fg_thread = if !fg_hwnd.is_null() {
                    GetWindowThreadProcessId(fg_hwnd, std::ptr::null_mut())
                } else {
                    0
                };

                if fg_thread != 0 && target_thread != 0 && fg_thread != target_thread {
                    AttachThreadInput(target_thread, fg_thread, 1);
                    if current_thread != target_thread {
                        AttachThreadInput(current_thread, target_thread, 1);
                    }
                    BringWindowToTop(h);
                    SetForegroundWindow(h);
                    SetFocus(h);
                    if current_thread != target_thread {
                        AttachThreadInput(current_thread, target_thread, 0);
                    }
                    AttachThreadInput(target_thread, fg_thread, 0);
                } else {
                    BringWindowToTop(h);
                    SetForegroundWindow(h);
                    SetFocus(h);
                }
            }
            apply_rounded_corners(hwnd);
        }
    }

    let lock = EGUI_CTX.lock().unwrap();
    if let Some(ctx) = lock.as_ref() {
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        ctx.request_repaint();
    }
}

pub fn hide_window() {
    log::info!("Coordinator: hide_window [START]");
    IS_VISIBLE.store(false, Ordering::SeqCst);
    SHOW_SETTINGS.store(false, Ordering::SeqCst);
    let rev = REVISION.fetch_add(1, Ordering::SeqCst) + 1;
    log::info!("Coordinator: hide_window revision incremented to {}", rev);

    #[cfg(target_os = "windows")]
    {
        let hwnd = ensure_main_hwnd();
        if hwnd != 0 {
            unsafe {
                use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};
                let h = hwnd as windows_sys::Win32::Foundation::HWND;
                ShowWindow(h, SW_HIDE);
            }
        }
    }

    let lock = EGUI_CTX.lock().unwrap();
    if let Some(ctx) = lock.as_ref() {
        ctx.request_repaint();
    }
    log::info!("Coordinator: hide_window [DONE]");
}

pub fn toggle_window() {
    {
        let mut last = LAST_TOGGLE.lock().unwrap();
        let now = Instant::now();
        if let Some(prev) = *last {
            if now.duration_since(prev) < Duration::from_millis(250) {
                log::info!("Coordinator: toggle_window debounced (<250ms)");
                return;
            }
        }
        *last = Some(now);
    }

    if is_visible() {
        hide_window();
    } else {
        show_window(false);
    }
}

pub fn trigger_rescan() {
    log::info!("Coordinator: trigger_rescan");
    SHOULD_RESCAN.store(true, Ordering::SeqCst);
    let lock = EGUI_CTX.lock().unwrap();
    if let Some(ctx) = lock.as_ref() {
        ctx.request_repaint();
    }
}

pub fn quit_app() {
    log::info!("Coordinator: quit_app");
    SHOULD_QUIT.store(true, Ordering::SeqCst);
    let lock = EGUI_CTX.lock().unwrap();
    if let Some(ctx) = lock.as_ref() {
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        ctx.request_repaint();
    }
    #[cfg(target_os = "windows")]
    {
        let hwnd = MAIN_HWND.load(Ordering::SeqCst);
        if hwnd != 0 {
            unsafe {
                use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};
                PostMessageW(hwnd as windows_sys::Win32::Foundation::HWND, WM_CLOSE, 0, 0);
            }
        }
    }
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_millis(150));
        std::process::exit(0);
    });
}
