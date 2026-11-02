use eframe::egui;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread;

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum TrayMessage {
    Open,
    Settings,
    Rescan,
    Quit,
    Toggle,
}

pub struct TrayService {
    #[allow(dead_code)]
    pub rx: Receiver<TrayMessage>,
    running: Arc<AtomicBool>,
}

impl Drop for TrayService {
    fn drop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
    }
}

impl TrayService {
    pub fn new(ctx: egui::Context) -> Result<Self, Box<dyn std::error::Error>> {
        let (tx, rx) = channel();
        let running = Arc::new(AtomicBool::new(true));
        let thread_running = running.clone();

        thread::Builder::new()
            .name("jlauncher-tray".to_string())
            .spawn(move || {
                #[cfg(target_os = "windows")]
                run_windows_tray(tx, thread_running, ctx);

                #[cfg(not(target_os = "windows"))]
                run_unix_tray(tx, thread_running, ctx);
            })?;

        Ok(Self { rx, running })
    }
}

#[cfg(target_os = "windows")]
unsafe extern "system" fn low_level_keyboard_proc(
    n_code: i32,
    w_param: windows_sys::Win32::Foundation::WPARAM,
    l_param: windows_sys::Win32::Foundation::LPARAM,
) -> windows_sys::Win32::Foundation::LRESULT {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_LMENU, VK_MENU, VK_RMENU, VK_SPACE,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, KBDLLHOOKSTRUCT, WM_KEYDOWN, WM_SYSKEYDOWN,
    };

    if n_code >= 0 && (w_param == WM_KEYDOWN as usize || w_param == WM_SYSKEYDOWN as usize) {
        let kbd = *(l_param as *const KBDLLHOOKSTRUCT);
        if kbd.vkCode == VK_SPACE as u32 && (kbd.flags & 0x80) == 0 {
            let alt_down = (kbd.flags & 0x20) != 0
                || (GetAsyncKeyState(VK_MENU as i32) as u16 & 0x8000) != 0
                || (GetAsyncKeyState(VK_LMENU as i32) as u16 & 0x8000) != 0
                || (GetAsyncKeyState(VK_RMENU as i32) as u16 & 0x8000) != 0;

            if alt_down {
                log::info!("Low-level hook: Alt+Space intercepted! Toggling launcher");
                crate::coordinator::toggle_window();
                return 1; // Swallow the key event so system window menu does not pop up
            }
        }
    }

    CallNextHookEx(std::ptr::null_mut(), n_code, w_param, l_param)
}

#[cfg(target_os = "windows")]
unsafe fn handle_tray_event(hwnd: windows_sys::Win32::Foundation::HWND, event: u32) {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    const WM_LBUTTONUP: u32 = 0x0202;
    const WM_LBUTTONDBLCLK: u32 = 0x0203;
    const WM_RBUTTONUP: u32 = 0x0205;
    const NIN_SELECT: u32 = 0x0400;
    const NIN_KEYSELECT: u32 = 0x0401;

    const IDM_OPEN: usize = 1;
    const IDM_SETTINGS: usize = 2;
    const IDM_RESCAN: usize = 3;
    const IDM_QUIT: usize = 4;

    if event == WM_LBUTTONUP || event == WM_LBUTTONDBLCLK || event == NIN_SELECT || event == NIN_KEYSELECT {
        log::info!("Native tray: Left click / select -> Show launcher");
        crate::coordinator::show_window(false);
    } else if event == WM_RBUTTONUP {
        log::info!("Native tray: Right click -> Showing popup menu");
        let hmenu = CreatePopupMenu();
        if hmenu.is_null() {
            return;
        }

        let open_text: Vec<u16> = "Open Launcher / Показать лаунчер\0".encode_utf16().collect();
        let settings_text: Vec<u16> = "Settings / Настройки...\0".encode_utf16().collect();
        let rescan_text: Vec<u16> = "Rescan Apps / Обновить программы\0".encode_utf16().collect();
        let quit_text: Vec<u16> = "Quit / Выход\0".encode_utf16().collect();

        AppendMenuW(hmenu, MF_STRING, IDM_OPEN, open_text.as_ptr());
        AppendMenuW(hmenu, MF_STRING, IDM_SETTINGS, settings_text.as_ptr());
        AppendMenuW(hmenu, MF_STRING, IDM_RESCAN, rescan_text.as_ptr());
        AppendMenuW(hmenu, MF_SEPARATOR, 0, std::ptr::null());
        AppendMenuW(hmenu, MF_STRING, IDM_QUIT, quit_text.as_ptr());

        let mut pt = POINT { x: 0, y: 0 };
        GetCursorPos(&mut pt);
        SetForegroundWindow(hwnd);

        let cmd = TrackPopupMenu(
            hmenu,
            TPM_LEFTALIGN | TPM_BOTTOMALIGN | TPM_RIGHTBUTTON | TPM_RETURNCMD,
            pt.x,
            pt.y,
            0,
            hwnd,
            std::ptr::null(),
        );
        PostMessageW(hwnd, WM_NULL, 0, 0);
        DestroyMenu(hmenu);

        let err = windows_sys::Win32::Foundation::GetLastError();
        log::info!("TrackPopupMenu selected cmd: {}, GetLastError: {}", cmd, err);
        match cmd as usize {
            IDM_OPEN => {
                crate::coordinator::show_window(false);
            }
            IDM_SETTINGS => {
                crate::coordinator::show_window(true);
            }
            IDM_RESCAN => {
                crate::coordinator::trigger_rescan();
            }
            IDM_QUIT => {
                use windows_sys::Win32::UI::Shell::{Shell_NotifyIconW, NIM_DELETE, NOTIFYICONDATAW};
                let mut nid = NOTIFYICONDATAW {
                    cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                    hWnd: hwnd,
                    uID: 1001,
                    ..std::mem::zeroed()
                };
                Shell_NotifyIconW(NIM_DELETE, &mut nid);
                crate::coordinator::quit_app();
            }
            _ => {}
        }
    }
}

#[cfg(target_os = "windows")]
fn run_windows_tray(_tx: Sender<TrayMessage>, running: Arc<AtomicBool>, _ctx: egui::Context) {
    use std::mem::size_of;
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::System::StationsAndDesktops::{
        OpenDesktopW, OpenWindowStationW, SetProcessWindowStation, SetThreadDesktop,
    };
    use windows_sys::Win32::UI::Shell::{
        Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    unsafe {
        const MAXIMUM_ALLOWED: u32 = 0x02000000;
        let winsta_name: Vec<u16> = "WinSta0\0".encode_utf16().collect();
        let hwinsta = OpenWindowStationW(winsta_name.as_ptr(), 0, MAXIMUM_ALLOWED);
        if !hwinsta.is_null() {
            SetProcessWindowStation(hwinsta);
        }
        let desk_name: Vec<u16> = "Default\0".encode_utf16().collect();
        let hdesk = OpenDesktopW(desk_name.as_ptr(), 0, 0, MAXIMUM_ALLOWED);
        if !hdesk.is_null() {
            SetThreadDesktop(hdesk);
        }

        let hinstance = GetModuleHandleW(std::ptr::null());
        let class_name: Vec<u16> = "JLauncher_TrayWndClass\0".encode_utf16().collect();

        const WM_TRAYICON: u32 = WM_USER + 100;

        unsafe extern "system" fn tray_wndproc(
            hwnd: HWND,
            msg: u32,
            wparam: WPARAM,
            lparam: LPARAM,
        ) -> LRESULT {
            if msg == WM_TRAYICON {
                let event = (lparam as u32) & 0xffff;
                log::info!("Native tray_wndproc: WM_TRAYICON event 0x{:04x}", event);
                handle_tray_event(hwnd, event);
                return 0;
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }

        let wnd_class = WNDCLASSW {
            lpfnWndProc: Some(tray_wndproc),
            hInstance: hinstance,
            lpszClassName: class_name.as_ptr(),
            ..std::mem::zeroed()
        };
        RegisterClassW(&wnd_class);

        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            class_name.as_ptr(),
            std::ptr::null(),
            WS_POPUP,
            0,
            0,
            0,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            hinstance,
            std::ptr::null_mut(),
        );
        ShowWindow(hwnd, SW_SHOW);

        let hook = SetWindowsHookExW(
            WH_KEYBOARD_LL,
            Some(low_level_keyboard_proc),
            hinstance,
            0,
        );
        log::info!("Low-level keyboard hook (WH_KEYBOARD_LL) installed: {:?}", hook);

        let hicon = create_native_hicon();

        let mut tip = [0u16; 128];
        let tip_str: Vec<u16> = "JLauncher\0".encode_utf16().collect();
        for (i, &c) in tip_str.iter().take(127).enumerate() {
            tip[i] = c;
        }

        let mut nid = NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: 1001,
            uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
            uCallbackMessage: WM_TRAYICON,
            hIcon: hicon,
            szTip: tip,
            ..std::mem::zeroed()
        };

        let add_res = Shell_NotifyIconW(NIM_ADD, &mut nid);
        log::info!("Native Shell_NotifyIconW NIM_ADD result: {}", add_res);

        // Allow messages through UIPI in case Explorer runs at different integrity
        ChangeWindowMessageFilterEx(hwnd, WM_TRAYICON, MSGFLT_ALLOW, std::ptr::null_mut());
        ChangeWindowMessageFilterEx(hwnd, WM_COMMAND, MSGFLT_ALLOW, std::ptr::null_mut());

        let mut msg: MSG = std::mem::zeroed();

        while running.load(Ordering::SeqCst) {
            while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                if msg.message == WM_QUIT {
                    break;
                }
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            MsgWaitForMultipleObjects(0, std::ptr::null(), 0, 40, QS_ALLINPUT);
        }

        if !hook.is_null() {
            UnhookWindowsHookEx(hook);
        }
        Shell_NotifyIconW(NIM_DELETE, &mut nid);
        DestroyIcon(hicon);
        DestroyWindow(hwnd);
        log::info!("Native Windows tray thread exited cleanly");
    }
}

#[cfg(target_os = "windows")]
unsafe fn create_native_hicon() -> windows_sys::Win32::UI::WindowsAndMessaging::HICON {
    use windows_sys::Win32::Graphics::Gdi::{CreateBitmap, DeleteObject};
    use windows_sys::Win32::UI::WindowsAndMessaging::{CreateIconIndirect, ICONINFO};

    const WIDTH: u32 = 32;
    const HEIGHT: u32 = 32;
    let mut rgba = [0u8; (WIDTH * HEIGHT * 4) as usize];

    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let dx = (x as f32) - 15.5;
            let dy = (y as f32) - 15.5;
            let dist = (dx * dx + dy * dy).sqrt();
            let idx = ((y * WIDTH + x) * 4) as usize;

            if dist <= 14.5 {
                let is_center_dot = dist <= 4.0;
                let is_ring = dist >= 7.0 && dist <= 10.0;
                let is_handle = x >= 18 && y >= 18 && (x as i32 - y as i32).abs() <= 2 && dist <= 13.0;

                if is_center_dot || is_handle {
                    // Crisp pure white (BGRA for Win32 DIB: B, G, R, A)
                    rgba[idx] = 255;
                    rgba[idx + 1] = 255;
                    rgba[idx + 2] = 255;
                    rgba[idx + 3] = 255;
                } else if is_ring {
                    // Bright electric cyan #38BDF8
                    rgba[idx] = 248;
                    rgba[idx + 1] = 189;
                    rgba[idx + 2] = 56;
                    rgba[idx + 3] = 255;
                } else if dist <= 13.0 {
                    // Deep vivid blue #0284C7
                    rgba[idx] = 199;
                    rgba[idx + 1] = 132;
                    rgba[idx + 2] = 2;
                    rgba[idx + 3] = 255;
                } else {
                    // Outer border
                    rgba[idx] = 161;
                    rgba[idx + 1] = 105;
                    rgba[idx + 2] = 3;
                    rgba[idx + 2] = 230;
                }
            }
        }
    }

    let hbm_color = CreateBitmap(WIDTH as i32, HEIGHT as i32, 1, 32, rgba.as_ptr() as _);
    let hbm_mask = CreateBitmap(WIDTH as i32, HEIGHT as i32, 1, 1, std::ptr::null());

    let mut icon_info = ICONINFO {
        fIcon: 1, // TRUE = icon
        xHotspot: 0,
        yHotspot: 0,
        hbmMask: hbm_mask,
        hbmColor: hbm_color,
    };

    let hicon = CreateIconIndirect(&mut icon_info);
    DeleteObject(hbm_color as _);
    DeleteObject(hbm_mask as _);
    hicon
}

#[cfg(not(target_os = "windows"))]
fn run_unix_tray(tx: Sender<TrayMessage>, running: Arc<AtomicBool>, ctx: egui::Context) {
    use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
    use tray_icon::{Icon, TrayIconBuilder, TrayIconEvent};

    let menu = Menu::new();

    let item_open = MenuItem::new("Open Launcher / Показать лаунчер", true, None);
    let item_settings = MenuItem::new("Settings / Настройки...", true, None);
    let item_rescan = MenuItem::new("Rescan Apps / Обновить программы", true, None);
    let item_quit = MenuItem::new("Quit / Выход", true, None);

    let open_id = item_open.id().0.clone();
    let settings_id = item_settings.id().0.clone();
    let rescan_id = item_rescan.id().0.clone();
    let quit_id = item_quit.id().0.clone();

    let _ = menu.append(&item_open);
    let _ = menu.append(&item_settings);
    let _ = menu.append(&item_rescan);
    let _ = menu.append(&PredefinedMenuItem::separator());
    let _ = menu.append(&item_quit);

    const WIDTH: u32 = 32;
    const HEIGHT: u32 = 32;
    let mut rgba = Vec::with_capacity((WIDTH * HEIGHT * 4) as usize);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let dx = (x as f32) - 15.5;
            let dy = (y as f32) - 15.5;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist <= 14.5 {
                if dist <= 4.0 || (x >= 18 && y >= 18 && (x as i32 - y as i32).abs() <= 2 && dist <= 13.0) {
                    rgba.extend_from_slice(&[255, 255, 255, 255]);
                } else if dist >= 7.0 && dist <= 10.0 {
                    rgba.extend_from_slice(&[56, 189, 248, 255]);
                } else {
                    rgba.extend_from_slice(&[2, 132, 199, 255]);
                }
            } else {
                rgba.extend_from_slice(&[0, 0, 0, 0]);
            }
        }
    }

    let Ok(icon) = Icon::from_rgba(rgba, WIDTH, HEIGHT) else {
        return;
    };

    let _tray_icon = match TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("JLauncher")
        .with_icon(icon)
        .build()
    {
        Ok(t) => t,
        Err(e) => {
            log::error!("Failed to build TrayIcon on Unix: {:?}", e);
            return;
        }
    };

    while running.load(Ordering::SeqCst) {
        std::thread::sleep(std::time::Duration::from_millis(30));

        while let Ok(event) = TrayIconEvent::receiver().try_recv() {
            if let TrayIconEvent::Click { .. } = event {
                crate::coordinator::toggle_window();
                let _ = tx.send(TrayMessage::Toggle);
                ctx.request_repaint();
            }
        }

        while let Ok(event) = MenuEvent::receiver().try_recv() {
            let id_str = event.id.0.as_str();
            if id_str == open_id {
                crate::coordinator::show_window(false);
                let _ = tx.send(TrayMessage::Open);
                ctx.request_repaint();
            } else if id_str == settings_id {
                crate::coordinator::show_window(true);
                let _ = tx.send(TrayMessage::Settings);
                ctx.request_repaint();
            } else if id_str == rescan_id {
                crate::coordinator::trigger_rescan();
                let _ = tx.send(TrayMessage::Rescan);
                ctx.request_repaint();
            } else if id_str == quit_id {
                crate::coordinator::quit_app();
                let _ = tx.send(TrayMessage::Quit);
                ctx.request_repaint();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tray_thread() {
        let ctx = egui::Context::default();
        let tray = TrayService::new(ctx).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(200));
        drop(tray);
    }
}
