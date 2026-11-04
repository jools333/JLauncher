use crate::apps::{AppEntry, AppIndex};
use crate::calculator::{CalcResult, Calculator};
use crate::config::Config;
use crate::hotkey::HotkeyService;
use crate::ipc::IpcCommand;
use crate::system::{execute_action, SystemAction, SYSTEM_ACTIONS};
use crate::tray::TrayService;
use crate::updater::{UpdateState, UpdaterService, CURRENT_VERSION};
use arboard::Clipboard;
use eframe::egui::{self, Color32, Key, Rounding, Stroke};
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use log::{error, info};
use std::sync::mpsc::Receiver;

#[derive(Debug, Clone)]
pub enum LauncherItem {
    Calculator(CalcResult),
    System(SystemAction),
    App(AppEntry),
}

fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    #[cfg(target_os = "windows")]
    {
        let font_candidates = [
            ("segoe_ui", "C:\\Windows\\Fonts\\segoeui.ttf", true),
            ("segoe_sym", "C:\\Windows\\Fonts\\seguisym.ttf", false),
        ];

        for (name, path, is_primary) in font_candidates {
            if let Ok(data) = std::fs::read(path) {
                fonts
                    .font_data
                    .insert(name.to_owned(), egui::FontData::from_owned(data));
                if is_primary {
                    fonts
                        .families
                        .entry(egui::FontFamily::Proportional)
                        .or_default()
                        .insert(0, name.to_owned());
                } else {
                    fonts
                        .families
                        .entry(egui::FontFamily::Proportional)
                        .or_default()
                        .push(name.to_owned());
                    fonts
                        .families
                        .entry(egui::FontFamily::Monospace)
                        .or_default()
                        .push(name.to_owned());
                }
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        let unix_candidates = [
            (
                "noto_sans",
                "/usr/share/fonts/opentype/noto/NotoSans-Regular.ttf",
            ),
            (
                "dejavu_sans",
                "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            ),
            (
                "noto_symbols",
                "/usr/share/fonts/truetype/noto/NotoSansSymbols-Regular.ttf",
            ),
        ];
        for (name, path) in unix_candidates {
            if let Ok(data) = std::fs::read(path) {
                fonts
                    .font_data
                    .insert(name.to_owned(), egui::FontData::from_owned(data));
                fonts
                    .families
                    .entry(egui::FontFamily::Proportional)
                    .or_default()
                    .push(name.to_owned());
            }
        }
    }

    ctx.set_fonts(fonts);
}

fn render_keycap(ui: &mut egui::Ui, text: &str) {
    let key_bg = Color32::from_rgb(38, 43, 56);
    let key_border = Color32::from_rgb(60, 68, 88);
    let key_text_color = Color32::from_rgb(210, 218, 235);

    egui::Frame::none()
        .fill(key_bg)
        .stroke(Stroke::new(1.0_f32, key_border))
        .rounding(Rounding::same(4.0))
        .inner_margin(egui::Margin::symmetric(5.0, 2.0))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(text)
                    .color(key_text_color)
                    .size(11.0)
                    .strong(),
            );
        });
}

fn is_window_focused(ctx: &egui::Context) -> bool {
    let egui_focused = ctx.input(|i| i.viewport().focused);
    #[cfg(target_os = "windows")]
    {
        let hwnd = crate::coordinator::get_main_hwnd();
        if hwnd != 0 {
            use windows_sys::Win32::System::Threading::GetCurrentProcessId;
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                GetAncestor, GetForegroundWindow, GetWindowThreadProcessId, GA_ROOT,
            };
            let fg = unsafe { GetForegroundWindow() };
            if !fg.is_null() {
                let h = hwnd as windows_sys::Win32::Foundation::HWND;
                if fg == h || unsafe { GetAncestor(fg, GA_ROOT) } == h {
                    return true;
                }
                let mut fg_pid = 0;
                unsafe { GetWindowThreadProcessId(fg, &mut fg_pid) };
                if fg_pid == unsafe { GetCurrentProcessId() } {
                    return true;
                }
                return false;
            }
        }
    }
    egui_focused.unwrap_or(true)
}

pub struct JLauncherApp {
    config: Config,
    query: String,
    selected_index: usize,
    scroll_to_selected: bool,
    app_index: AppIndex,
    calculator: Calculator,
    matcher: SkimMatcherV2,
    results: Vec<LauncherItem>,

    // Services
    hotkey_service: HotkeyService,
    _tray_service: Option<TrayService>,
    _ipc_rx: Receiver<IpcCommand>,
    updater: UpdaterService,

    // UI state
    is_visible: bool,
    show_settings: bool,
    status_toast: Option<(String, std::time::Instant)>,
    settings_hotkey_input: String,
    hotkey_error: Option<String>,
    should_focus_search: bool,
    should_quit: bool,
    last_revision: u64,
    was_focused: bool,
    visible_since: Option<std::time::Instant>,
}

impl JLauncherApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        ipc_rx: Receiver<IpcCommand>,
        start_hidden: bool,
    ) -> Self {
        // Setup custom dark style
        let mut style = (*cc.egui_ctx.style()).clone();
        style.visuals.dark_mode = true;
        style.visuals.window_rounding = Rounding::same(12.0);
        style.visuals.override_text_color = Some(Color32::from_rgb(230, 235, 245));
        cc.egui_ctx.set_style(style);

        setup_fonts(&cc.egui_ctx);

        crate::coordinator::init(cc.egui_ctx.clone(), start_hidden);

        let config = Config::load();
        let app_index = AppIndex::new();
        let calculator = Calculator::new();
        let matcher = SkimMatcherV2::default();

        let mut hotkey_service = HotkeyService::new();
        if let Err(e) = hotkey_service.register(&config.hotkey) {
            error!("Failed to register initial hotkey: {}", e);
        }

        global_hotkey::GlobalHotKeyEvent::set_event_handler(Some(
            move |event: global_hotkey::GlobalHotKeyEvent| {
                if event.state == global_hotkey::HotKeyState::Pressed {
                    crate::coordinator::toggle_window();
                }
            },
        ));

        let tray_service = match TrayService::new(cc.egui_ctx.clone()) {
            Ok(t) => Some(t),
            Err(e) => {
                error!("Failed to initialize tray service: {}", e);
                None
            }
        };

        let initial_hotkey = config.hotkey.clone();

        let updater = UpdaterService::new();
        if config.auto_check_updates {
            updater.check_in_background();
        }

        let mut app = Self {
            config,
            query: String::new(),
            selected_index: 0,
            scroll_to_selected: true,
            app_index,
            calculator,
            matcher,
            results: Vec::new(),
            hotkey_service,
            _tray_service: tray_service,
            _ipc_rx: ipc_rx,
            updater,
            is_visible: !start_hidden,
            show_settings: false,
            status_toast: None,
            settings_hotkey_input: initial_hotkey,
            hotkey_error: None,
            should_focus_search: true,
            should_quit: false,
            last_revision: 0,
            was_focused: false,
            visible_since: if start_hidden {
                None
            } else {
                Some(std::time::Instant::now())
            },
        };

        if !start_hidden {
            cc.egui_ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }

        app.update_search();
        app
    }

    #[cfg(target_os = "windows")]
    fn find_and_register_main_hwnd() {
        let hwnd = crate::coordinator::ensure_main_hwnd();
        if hwnd != 0 {
            crate::coordinator::apply_rounded_corners(hwnd);
        }
    }

    pub fn hide_window(&mut self, _ctx: &egui::Context) {
        info!("JLauncherApp::hide_window [START]");
        crate::coordinator::hide_window();
        #[cfg(not(target_os = "windows"))]
        _ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        self.is_visible = false;
        self.last_revision = crate::coordinator::get_revision();
        self.was_focused = false;
        self.visible_since = None;
        info!("JLauncherApp::hide_window [DONE]");
    }

    pub fn show_window(&mut self, ctx: &egui::Context) {
        info!("JLauncherApp::show_window [START]");
        crate::coordinator::show_window(self.show_settings);
        #[cfg(not(target_os = "windows"))]
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        self.is_visible = true;
        self.last_revision = crate::coordinator::get_revision();
        self.was_focused = false;
        self.visible_since = Some(std::time::Instant::now());

        if self.config.clear_on_hide && !self.is_calculator_query() {
            self.query.clear();
        }
        self.selected_index = 0;
        self.scroll_to_selected = true;
        self.should_focus_search = true;
        self.update_search();
        info!("JLauncherApp::show_window [DONE]");
    }

    pub fn set_visible(&mut self, ctx: &egui::Context, visible: bool) {
        if visible {
            self.show_window(ctx);
        } else {
            self.hide_window(ctx);
        }
    }

    #[allow(dead_code)]
    pub fn toggle_visibility(&mut self, ctx: &egui::Context) {
        let new_state = !self.is_visible;
        self.set_visible(ctx, new_state);
    }

    fn update_search(&mut self) {
        let query = self.query.trim();
        let mut items = Vec::new();

        // 1. Calculator
        if self.config.enable_calculator {
            if let Some(calc) = self.calculator.evaluate(query) {
                items.push(LauncherItem::Calculator(calc));
            }
        }

        // 2. System Commands
        if self.config.enable_system_commands {
            if !query.is_empty() {
                for action in SYSTEM_ACTIONS {
                    let mut matched = false;
                    if self.matcher.fuzzy_match(action.title, query).is_some() {
                        matched = true;
                    } else {
                        for kw in action.keywords {
                            if self.matcher.fuzzy_match(kw, query).is_some() {
                                matched = true;
                                break;
                            }
                        }
                    }

                    if matched {
                        items.push(LauncherItem::System(action.clone()));
                    }
                }
            }
        }

        // 3. Applications
        let app_limit = self.config.max_results.saturating_sub(items.len()).max(3);
        let app_matches = self.app_index.search(query, app_limit);
        for m in app_matches {
            items.push(LauncherItem::App(m.entry));
        }

        self.results = items;
        if self.selected_index >= self.results.len() {
            self.selected_index = self.results.len().saturating_sub(1);
        }
    }

    fn execute_selected(&mut self, ctx: &egui::Context) {
        if self.results.is_empty() {
            return;
        }

        match &self.results[self.selected_index].clone() {
            LauncherItem::Calculator(calc) => {
                // Copy result to clipboard
                if let Ok(mut cb) = Clipboard::new() {
                    let _ = cb.set_text(&calc.formatted_result);
                    self.show_toast(format!("Copied '{}' to clipboard!", calc.formatted_result));
                }
                self.hide_window(ctx);
            }
            LauncherItem::System(action) => {
                info!("Executing system action: {}", action.id);
                if let Err(e) = execute_action(action.id) {
                    self.show_toast(format!("Action error: {}", e));
                }
                self.hide_window(ctx);
            }
            LauncherItem::App(app) => {
                info!("Launching application: {}", app.name);
                if let Err(e) = AppIndex::launch(app) {
                    self.show_toast(format!("Failed to launch: {}", e));
                }
                self.hide_window(ctx);
            }
        }
    }

    fn is_calculator_query(&self) -> bool {
        let trimmed = self.query.trim();
        if trimmed.is_empty() {
            return false;
        }
        if trimmed.starts_with('=') {
            return true;
        }
        if self.calculator.evaluate(trimmed).is_some() {
            return true;
        }
        if self
            .results
            .iter()
            .any(|item| matches!(item, LauncherItem::Calculator(_)))
        {
            return true;
        }
        if (trimmed.ends_with('+')
            || trimmed.ends_with('-')
            || trimmed.ends_with('*')
            || trimmed.ends_with('/'))
            && trimmed.chars().any(|c| c.is_ascii_digit())
        {
            return true;
        }
        false
    }

    fn show_toast(&mut self, msg: String) {
        self.status_toast = Some((msg, std::time::Instant::now()));
    }
}

impl eframe::App for JLauncherApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        Color32::from_rgb(26, 29, 36).to_normalized_gamma_f32()
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        #[cfg(target_os = "windows")]
        Self::find_and_register_main_hwnd();

        if crate::coordinator::check_and_clear_rescan() {
            self.app_index.refresh();
            self.update_search();
            self.show_toast("Applications rescanned!".into());
        }

        if crate::coordinator::should_quit() || self.should_quit {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        // Intercept close requests (e.g. from window manager) and minimize/hide instead of exiting
        if ctx.input(|i| i.viewport().close_requested()) {
            if crate::coordinator::should_quit() || self.should_quit {
                // Allow exit
                return;
            } else {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.hide_window(ctx);
                return;
            }
        }

        let rev = crate::coordinator::get_revision();
        log::trace!(
            "App::update: frame start, rev={}, last_rev={}, is_visible={}",
            rev,
            self.last_revision,
            self.is_visible
        );
        if rev != self.last_revision {
            self.last_revision = rev;
            let coord_visible = crate::coordinator::is_visible();
            self.is_visible = coord_visible;
            self.show_settings = crate::coordinator::show_settings();
            info!(
                "App::update synced rev={}: is_visible={}, show_settings={}",
                rev, self.is_visible, self.show_settings
            );

            if self.is_visible {
                #[cfg(not(target_os = "windows"))]
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);

                #[cfg(target_os = "windows")]
                {
                    let hwnd = crate::coordinator::get_main_hwnd();
                    if hwnd != 0 {
                        unsafe {
                            use windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus;
                            use windows_sys::Win32::UI::WindowsAndMessaging::{
                                BringWindowToTop, SetForegroundWindow,
                            };
                            let h = hwnd as windows_sys::Win32::Foundation::HWND;
                            BringWindowToTop(h);
                            SetForegroundWindow(h);
                            SetFocus(h);
                        }
                    }
                }

                if self.config.clear_on_hide && !self.is_calculator_query() {
                    self.query.clear();
                }
                self.selected_index = 0;
                self.scroll_to_selected = true;
                self.should_focus_search = true;
                self.update_search();

                self.was_focused = false;
                self.visible_since = Some(std::time::Instant::now());
            } else {
                #[cfg(not(target_os = "windows"))]
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                self.was_focused = false;
                self.visible_since = None;
            }
        }

        if !self.is_visible {
            return;
        }

        // Only request periodic repaint when visible (for UI animations)
        ctx.request_repaint_after(std::time::Duration::from_millis(50));

        // Hide window when focus is lost (only if the window previously gained focus)
        let is_focused = is_window_focused(ctx);
        if is_focused {
            self.was_focused = true;
        } else if self.was_focused {
            let grace_passed = self.visible_since.map_or(true, |t| {
                t.elapsed() >= std::time::Duration::from_millis(200)
            });
            if grace_passed {
                info!("Launcher window lost focus -> hiding window");
                self.hide_window(ctx);
                return;
            }
        }

        // Extract keyboard navigation inputs safely without holding ctx.input lock
        let (escape_pressed, down_pressed, up_pressed, enter_pressed, alt_s_pressed) =
            ctx.input(|i| {
                let down = i.key_pressed(Key::ArrowDown)
                    || (i.key_pressed(Key::Tab) && !i.modifiers.shift);
                let up = i.key_pressed(Key::ArrowUp)
                    || (i.key_pressed(Key::Tab) && i.modifiers.shift);
                (
                    i.key_pressed(Key::Escape),
                    down,
                    up,
                    i.key_pressed(Key::Enter),
                    i.modifiers.alt && i.key_pressed(Key::S),
                )
            });

        if escape_pressed {
            if self.show_settings {
                self.show_settings = false;
                crate::coordinator::set_show_settings(false);
                self.should_focus_search = true;
            } else {
                info!("Escape pressed -> hiding window");
                self.hide_window(ctx);
                return;
            }
        }

        if !self.show_settings {
            if down_pressed && !self.results.is_empty() {
                self.selected_index = (self.selected_index + 1).min(self.results.len() - 1);
                self.scroll_to_selected = true;
            }
            if up_pressed {
                self.selected_index = self.selected_index.saturating_sub(1);
                self.scroll_to_selected = true;
            }
            if enter_pressed {
                self.execute_selected(ctx);
                return;
            }
            if alt_s_pressed {
                self.show_settings = !self.show_settings;
                crate::coordinator::set_show_settings(self.show_settings);
                if !self.show_settings {
                    self.should_focus_search = true;
                }
            }
        }

        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(Color32::from_rgb(26, 29, 36))
                    .stroke(Stroke::new(1.2_f32, Color32::from_rgb(55, 62, 78)))
                    .rounding(Rounding::same(12.0))
                    .inner_margin(egui::Margin::same(14.0)),
            )
            .show(ctx, |ui| {
                if self.show_settings {
                    self.render_settings(ui, ctx);
                } else {
                    self.render_launcher(ui, ctx);
                }
            });
    }
}

impl JLauncherApp {
    fn render_launcher(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        // Search Input Bar
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("🔍").size(20.0));
            ui.add_space(4.0);

            let text_edit = egui::TextEdit::singleline(&mut self.query)
                .hint_text("Type an app name, math expression (=2+2), or command...")
                .font(egui::FontId::proportional(18.0))
                .text_color(Color32::WHITE)
                .frame(false)
                .desired_width(ui.available_width() - 70.0);

            let response = ui.add(text_edit);
            if self.should_focus_search || !response.has_focus() {
                response.request_focus();
                if response.has_focus() {
                    self.should_focus_search = false;
                }
            }

            if response.changed() {
                self.selected_index = 0;
                self.scroll_to_selected = true;
                self.update_search();
            }

            // Settings button
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(egui::RichText::new("⚙").size(16.0)).clicked() {
                    self.show_settings = true;
                    crate::coordinator::set_show_settings(true);
                }
            });
        });

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(6.0);

        // Results List
        egui::ScrollArea::vertical()
            .max_height(320.0)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if self.results.is_empty() {
                    ui.add_space(20.0);
                    ui.vertical_centered(|ui| {
                        ui.label(
                            egui::RichText::new("No matching applications or commands")
                                .color(Color32::from_rgb(140, 145, 160))
                                .size(14.0),
                        );
                    });
                } else {
                    let mouse_moved = ctx.input(|i| i.pointer.delta().length_sq() > 1.0);
                    let mut clicked_index = None;
                    let mut hovered_index = None;

                    for (index, item) in self.results.iter().enumerate() {
                        let is_selected = index == self.selected_index;
                        let row_response = self.render_result_row(ui, item, is_selected);

                        if is_selected && self.scroll_to_selected {
                            row_response.scroll_to_me(Some(egui::Align::Center));
                        }

                        if row_response.clicked() {
                            clicked_index = Some(index);
                        } else if mouse_moved && row_response.hovered() {
                            hovered_index = Some(index);
                        }
                        ui.add_space(2.0);
                    }

                    self.scroll_to_selected = false;

                    if let Some(index) = hovered_index {
                        self.selected_index = index;
                    }

                    if let Some(index) = clicked_index {
                        self.selected_index = index;
                        self.execute_selected(ctx);
                    }
                }
            });

        // Bottom status / hint bar
        ui.add_space(6.0);
        ui.separator();
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            if let Some((msg, time)) = &self.status_toast {
                if time.elapsed().as_secs() < 3 {
                    ui.label(
                        egui::RichText::new(msg)
                            .color(Color32::from_rgb(52, 211, 153))
                            .size(12.0),
                    );
                } else {
                    self.render_hotkey_hints(ui);
                }
            } else {
                self.render_hotkey_hints(ui);
            }
        });
    }

    fn render_hotkey_hints(&mut self, ui: &mut egui::Ui) {
        let hint_color = Color32::from_rgb(140, 148, 165);

        render_keycap(ui, "↑");
        ui.add_space(2.0);
        render_keycap(ui, "↓");
        ui.add_space(4.0);
        ui.label(egui::RichText::new("Navigate").color(hint_color).size(11.0));

        ui.add_space(10.0);

        render_keycap(ui, "↵ Enter");
        ui.add_space(4.0);
        ui.label(egui::RichText::new("Execute").color(hint_color).size(11.0));

        ui.add_space(10.0);

        render_keycap(ui, "Esc");
        ui.add_space(4.0);
        ui.label(egui::RichText::new("Close").color(hint_color).size(11.0));

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(egui::RichText::new("Settings").color(hint_color).size(11.0));
            ui.add_space(4.0);
            render_keycap(ui, "Alt+S");

            match self.updater.state() {
                UpdateState::Available(info) => {
                    ui.add_space(10.0);
                    if ui
                        .link(
                            egui::RichText::new(format!("✨ Update {} available", info.version))
                                .color(Color32::from_rgb(96, 165, 250))
                                .size(11.0),
                        )
                        .clicked()
                    {
                        self.show_settings = true;
                        crate::coordinator::set_show_settings(true);
                    }
                }
                UpdateState::ReadyToRestart => {
                    ui.add_space(10.0);
                    if ui
                        .link(
                            egui::RichText::new("🔄 Restart to apply update")
                                .color(Color32::from_rgb(52, 211, 153))
                                .size(11.0),
                        )
                        .clicked()
                    {
                        let _ = crate::updater::UpdaterService::restart();
                    }
                }
                _ => {}
            }
        });
    }

    fn render_result_row(
        &self,
        ui: &mut egui::Ui,
        item: &LauncherItem,
        is_selected: bool,
    ) -> egui::Response {
        let bg = if is_selected {
            Color32::from_rgb(42, 60, 96)
        } else {
            Color32::TRANSPARENT
        };

        let stroke = if is_selected {
            Stroke::new(1.0_f32, Color32::from_rgb(66, 120, 220))
        } else {
            Stroke::NONE
        };

        let frame_resp = egui::Frame::none()
            .fill(bg)
            .stroke(stroke)
            .rounding(Rounding::same(8.0))
            .inner_margin(egui::Margin::symmetric(10.0, 6.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| match item {
                    LauncherItem::Calculator(calc) => {
                        ui.label(egui::RichText::new("🧮").size(18.0));
                        ui.add_space(6.0);
                        ui.vertical(|ui| {
                            ui.label(
                                egui::RichText::new(&calc.formatted_result)
                                    .color(Color32::from_rgb(52, 211, 153))
                                    .strong()
                                    .size(16.0),
                            );
                            ui.label(
                                egui::RichText::new(format!(
                                    "Calculator: {} (Enter to copy)",
                                    calc.expression
                                ))
                                .color(Color32::from_rgb(160, 168, 185))
                                .size(12.0),
                            );
                        });
                    }
                    LauncherItem::System(action) => {
                        ui.label(egui::RichText::new(action.icon).size(18.0));
                        ui.add_space(6.0);
                        ui.vertical(|ui| {
                            ui.label(
                                egui::RichText::new(action.title)
                                    .color(Color32::from_rgb(248, 113, 113))
                                    .strong()
                                    .size(15.0),
                            );
                            ui.label(
                                egui::RichText::new(action.description)
                                    .color(Color32::from_rgb(160, 168, 185))
                                    .size(12.0),
                            );
                        });
                    }
                    LauncherItem::App(app) => {
                        let icon = app.icon.as_deref().unwrap_or("🚀");
                        ui.label(egui::RichText::new(icon).size(18.0));
                        ui.add_space(6.0);
                        ui.vertical(|ui| {
                            ui.label(
                                egui::RichText::new(&app.name)
                                    .color(Color32::WHITE)
                                    .strong()
                                    .size(15.0),
                            );
                            if !app.description.is_empty() {
                                ui.label(
                                    egui::RichText::new(&app.description)
                                        .color(Color32::from_rgb(150, 158, 175))
                                        .size(12.0),
                                );
                            }
                        });
                    }
                });
            });

        frame_resp
            .response
            .interact(egui::Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand)
    }

    fn render_settings(&mut self, ui: &mut egui::Ui, _ctx: &egui::Context) {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("⚙ Settings / Настройки")
                    .size(18.0)
                    .strong()
                    .color(Color32::WHITE),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("✕ Close").clicked() {
                    self.show_settings = false;
                    crate::coordinator::set_show_settings(false);
                    self.should_focus_search = true;
                }
            });
        });

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);

        egui::ScrollArea::vertical()
            .max_height(360.0)
            .show(ui, |ui| {
                // Hotkey configuration
                ui.label(egui::RichText::new("Global Shortcut / Горячая клавиша:").strong());
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.settings_hotkey_input)
                            .desired_width(180.0)
                            .hint_text("e.g. Alt+Space, Ctrl+Space"),
                    );

                    if ui.button("Apply Hotkey / Применить").clicked() {
                        let trimmed = self.settings_hotkey_input.trim().to_string();
                        match self.hotkey_service.register(&trimmed) {
                            Ok(_) => {
                                self.config.hotkey = trimmed;
                                let _ = self.config.save();
                                self.hotkey_error = None;
                                self.show_toast("Hotkey updated!".into());
                            }
                            Err(e) => {
                                self.hotkey_error = Some(e);
                            }
                        }
                    }
                });

                if let Some(err) = &self.hotkey_error {
                    ui.label(egui::RichText::new(err).color(Color32::from_rgb(248, 113, 113)).size(12.0));
                }

                ui.label(
                    egui::RichText::new("Tip: On Linux Wayland, you can also bind 'jlauncher --toggle' in your desktop shortcut settings.")
                        .color(Color32::from_rgb(140, 148, 165))
                        .size(11.0),
                );

                ui.add_space(12.0);

                // Autostart configuration
                let mut autostart = self.config.autostart;
                if ui.checkbox(&mut autostart, "Launch on system startup / Автозапуск при включении").changed() {
                    self.config.autostart = autostart;
                    if let Err(e) = self.config.sync_autostart() {
                        error!("Failed to sync autostart: {}", e);
                    }
                    let _ = self.config.save();
                }

                ui.add_space(8.0);

                // Feature toggles
                let mut calc_enabled = self.config.enable_calculator;
                if ui.checkbox(&mut calc_enabled, "Enable Calculator / Включить калькулятор").changed() {
                    self.config.enable_calculator = calc_enabled;
                    let _ = self.config.save();
                    self.update_search();
                }

                let mut sys_enabled = self.config.enable_system_commands;
                if ui.checkbox(&mut sys_enabled, "Enable Power Commands (Shutdown/Reboot) / Команды выключения").changed() {
                    self.config.enable_system_commands = sys_enabled;
                    let _ = self.config.save();
                    self.update_search();
                }

                let mut clear_on_hide = self.config.clear_on_hide;
                if ui.checkbox(&mut clear_on_hide, "Clear search query on hide / Очищать поле при скрытии").changed() {
                    self.config.clear_on_hide = clear_on_hide;
                    let _ = self.config.save();
                }

                ui.add_space(8.0);

                // Max results slider
                ui.horizontal(|ui| {
                    ui.label("Max search results / Макс. результатов:");
                    let mut max_res = self.config.max_results;
                    if ui.add(egui::Slider::new(&mut max_res, 3..=15)).changed() {
                        self.config.max_results = max_res;
                        let _ = self.config.save();
                        self.update_search();
                    }
                });

                ui.add_space(14.0);

                // Rescan apps button
                if ui.button("🔄 Rescan Applications Now / Обновить список программ").clicked() {
                    self.app_index.refresh();
                    self.update_search();
                    self.show_toast("Applications rescanned!".into());
                }

                ui.add_space(14.0);
                ui.separator();
                ui.add_space(8.0);

                // Auto-updates section
                ui.label(
                    egui::RichText::new("🚀 Updates / Обновления")
                        .size(15.0)
                        .strong()
                        .color(Color32::WHITE),
                );
                ui.add_space(4.0);

                let mut auto_check = self.config.auto_check_updates;
                if ui
                    .checkbox(
                        &mut auto_check,
                        "Check for updates automatically / Автопроверка обновлений при запуске",
                    )
                    .changed()
                {
                    self.config.auto_check_updates = auto_check;
                    let _ = self.config.save();
                }

                ui.add_space(6.0);

                let update_state = self.updater.state();
                match &update_state {
                    UpdateState::Idle => {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(format!(
                                    "Current version: v{}",
                                    CURRENT_VERSION
                                ))
                                .color(Color32::from_rgb(160, 168, 185)),
                            );
                            if ui.button("🔍 Check for Updates / Проверить").clicked() {
                                self.updater.check_in_background();
                            }
                        });
                    }
                    UpdateState::Checking => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("Checking for updates / Поиск обновлений...");
                        });
                    }
                    UpdateState::UpToDate => {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(format!(
                                    "✓ Up to date (v{}) / У вас актуальная версия",
                                    CURRENT_VERSION
                                ))
                                .color(Color32::from_rgb(52, 211, 153)),
                            );
                            if ui.button("Check again / Проверить снова").clicked() {
                                self.updater.check_in_background();
                            }
                        });
                    }
                    UpdateState::Available(info) => {
                        egui::Frame::none()
                            .fill(Color32::from_rgb(26, 36, 56))
                            .stroke(Stroke::new(1.0_f32, Color32::from_rgb(59, 130, 246)))
                            .rounding(Rounding::same(8.0))
                            .inner_margin(egui::Margin::symmetric(10.0, 8.0))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "🎉 New version available: {}",
                                            info.version
                                        ))
                                        .strong()
                                        .color(Color32::from_rgb(96, 165, 250)),
                                    );
                                    let size_mb = info.asset_size as f64 / (1024.0 * 1024.0);
                                    ui.label(
                                        egui::RichText::new(format!("({:.1} MB)", size_mb))
                                            .color(Color32::from_rgb(160, 168, 185))
                                            .size(12.0),
                                    );
                                });

                                if !info.release_notes.is_empty() {
                                    ui.add_space(4.0);
                                    egui::CollapsingHeader::new("Changelog / Что нового")
                                        .id_salt("update_notes")
                                        .default_open(false)
                                        .show(ui, |ui| {
                                            egui::ScrollArea::vertical()
                                                .max_height(100.0)
                                                .show(ui, |ui| {
                                                    ui.label(
                                                        egui::RichText::new(&info.release_notes)
                                                            .color(Color32::from_rgb(
                                                                200, 208, 225,
                                                            ))
                                                            .size(12.0),
                                                    );
                                                });
                                        });
                                }

                                ui.add_space(6.0);
                                ui.horizontal(|ui| {
                                    if ui
                                        .button(
                                            egui::RichText::new(
                                                "⬇ Download & Install / Скачать и обновить",
                                            )
                                            .strong(),
                                        )
                                        .clicked()
                                    {
                                        self.updater.download_and_install_in_background();
                                    }
                                    if ui.button("Check again / Проверить снова").clicked() {
                                        self.updater.check_in_background();
                                    }
                                });
                            });
                    }
                    UpdateState::Downloading {
                        progress,
                        downloaded,
                        total,
                    } => {
                        let dl_mb = *downloaded as f64 / (1024.0 * 1024.0);
                        let tot_mb = *total as f64 / (1024.0 * 1024.0);
                        let pct = (*progress * 100.0) as u32;

                        ui.label(format!(
                            "Downloading update: {:.1} MB / {:.1} MB ({}%)",
                            dl_mb, tot_mb, pct
                        ));
                        ui.add(
                            egui::ProgressBar::new(*progress)
                                .show_percentage()
                                .animate(true),
                        );
                    }
                    UpdateState::ReadyToRestart => {
                        egui::Frame::none()
                            .fill(Color32::from_rgb(20, 45, 30))
                            .stroke(Stroke::new(1.0_f32, Color32::from_rgb(52, 211, 153)))
                            .rounding(Rounding::same(8.0))
                            .inner_margin(egui::Margin::symmetric(10.0, 8.0))
                            .show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new(
                                        "✓ Update installed successfully! / Обновление успешно установлено!",
                                    )
                                    .strong()
                                    .color(Color32::from_rgb(52, 211, 153)),
                                );
                                ui.add_space(4.0);
                                if ui
                                    .button(
                                        egui::RichText::new(
                                            "🔄 Restart Now / Перезапустить сейчас",
                                        )
                                        .strong(),
                                    )
                                    .clicked()
                                {
                                    if let Err(e) = crate::updater::UpdaterService::restart() {
                                        self.show_toast(format!("Restart failed: {}", e));
                                    }
                                }
                            });
                    }
                    UpdateState::Error(err) => {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(format!("Update check failed: {}", err))
                                    .color(Color32::from_rgb(248, 113, 113))
                                    .size(12.0),
                            );
                            if ui.button("Retry / Повторить").clicked() {
                                self.updater.check_in_background();
                            }
                        });
                    }
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_calculator_query_detection() {
        let calc = Calculator::new();
        let check = |q: &str| -> bool {
            let trimmed = q.trim();
            if trimmed.is_empty() {
                return false;
            }
            if trimmed.starts_with('=') {
                return true;
            }
            if calc.evaluate(trimmed).is_some() {
                return true;
            }
            if (trimmed.ends_with('+')
                || trimmed.ends_with('-')
                || trimmed.ends_with('*')
                || trimmed.ends_with('/'))
                && trimmed.chars().any(|c| c.is_ascii_digit())
            {
                return true;
            }
            false
        };

        // Positive cases (calculator)
        assert!(check("2+2"));
        assert!(check(" 100 * 5 "));
        assert!(check("=42"));
        assert!(check("=anything"));
        assert!(check("12 + "));
        assert!(check("sqrt(16)"));
        assert!(check("15% of 200"));
        assert!(check("pi * 2"));

        // Negative cases (not calculator, should be cleared)
        assert!(!check(""));
        assert!(!check("   "));
        assert!(!check("notepad"));
        assert!(!check("calc"));
        assert!(!check("chrome"));
        assert!(!check("7-zip"));
    }
}
