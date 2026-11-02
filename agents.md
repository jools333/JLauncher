# AGENTS.md — JLauncher Architecture & Agent Context

Документ для сохранения полного контекста разработки, архитектурных решений, особенностей платформ (Windows/Linux) и инструкций для будущих сессий агентов.

---

## 1. О проекте (Project Overview)

**JLauncher** — быстрый, легковесный кроссплатформенный лаунчер для **Windows** и **Linux** (X11 и Wayland), написанный на **Rust**.
Является аналогом программ *Ulauncher* и *Albert*.

### Ключевой функционал:
1. **Быстрый вызов по хоткею**: По умолчанию `Alt+Space` открывает лаконичное окно поиска по центру экрана.
2. **Встроенный калькулятор**: Вычисление математических выражений (`2+2`, `sqrt(16)`, `sin(pi/2)`, `50% of 200`, префиксы `=`, `?`) с копированием результата в буфер обмена по нажатию `Enter`.
3. **Поиск и запуск приложений**:
   - **Windows**: Индексация меню «Пуск» (`.lnk` ярлыки из `Common Programs` и `User Programs`), системных `.exe` из `PATH` и каталогов Windows.
   - **Linux**: Парсинг `.desktop` файлов (`/usr/share/applications`, `~/.local/share/applications` и др.).
   - Нечеткий поиск (Fuzzy matching) через алгоритм `SkimMatcherV2`.
4. **Управление питанием**: Команды завершения работы, перезагрузки, блокировки экрана, выхода из сеанса и спящего режима.
5. **Системный трей и настройки**: Иконка в системном трее с контекстным меню («Показать лаунчер», «Настройки...», «Обновить программы», «Выход»). Окно настроек позволяет изменить хоткей, включить/выключить автозагрузку (`auto-launch`), настроить калькулятор и очистку поля ввода.
6. **Одиночный экземпляр (Single Instance IPC)**: Локальный TCP-сервер (порт 41789) для межпроцессного взаимодействия и передачи команд (`--toggle`, `--show`, `--hide`, `--settings`, `--quit`).

---

## 2. Структура кодовой базы (Modules)

```
z:\jlauncher\
├── Cargo.toml               # Зависимости (eframe, egui, windows-sys, evalexpr, fuzzy-matcher и др.)
├── src/
│   ├── main.rs              # Точка входа, разбор CLI-аргументов, IPC-клиент/сервер, запуск eframe
│   ├── coordinator.rs       # Потокобезопасный координатор видимости и состояния окон
│   ├── app.rs               # Основной интерфейс eframe/egui, обработка ввода и клавиатурной навигации
│   ├── tray.rs              # Системный трей (нативный Win32 для Windows, tray-icon для Linux), WH_KEYBOARD_LL хук
│   ├── calculator.rs        # Модуль парсинга и вычисления математических выражений
│   ├── apps.rs              # Индексация установленных приложений (.lnk, .desktop, .exe)
│   ├── system.rs            # Выполнение системных команд (shutdown, reboot, lock, sleep)
│   ├── hotkey.rs            # Кроссплатформенный менеджер глобальных хоткеев (global-hotkey)
│   ├── ipc.rs               # TCP IPC сервер и клиент для передачи команд между экземплярами
│   └── config.rs            # Сохранение и загрузка конфигурации (JSON)
└── tests/                   # Набор модульных и интеграционных тестов
```

---

## 3. Критически важные платформенные особенности и грабли (Gotchas & Fixes)

### 3.1. Windows: Deadlock и «JLauncher (Не отвечает)» при скрытии окна в eframe/winit
- **Проблема**: Если скрыть окно лаунчера через `ViewportCommand::Visible(false)` в `eframe` (backend `winit`), `winit` на Windows выключает цикл генерации событий перерисовки `RedrawRequested` для этого окна. При последующей попытке вызвать окно через `ctx.request_repaint()` или `ViewportCommand::Visible(true)`, `winit` игнорирует запрос, окно не получает кадры, поток GUI перестает вычитывать системные сообщения Windows, и Desktop Window Manager (DWM) превращает его в «Ghost Window» с заголовком `(Не отвечает)`.
- **Решение**: На Windows **не** отправлять `ViewportCommand::Visible(false)` в движок `eframe`. Вместо этого в [coordinator.rs](file:///Z:/jlauncher/src/coordinator.rs):
  - При скрытии вызывать Win32 `ShowWindow(hwnd, SW_HIDE)`.
  - При показе вызывать `ShowWindow(hwnd, SW_RESTORE)` $\rightarrow$ `ShowWindow(hwnd, SW_SHOW)` $\rightarrow$ `SetWindowPos(hwnd, HWND_TOPMOST, x, y, 650, 440, SWP_SHOWWINDOW)` $\rightarrow$ `SetForegroundWindow(hwnd)`.
  - Windows сама присылает окну `WM_SHOWWINDOW` / `WM_PAINT`, `winit` получает их штатно и вызывает `App::update()`.
  - В `App::update()` при `!self.is_visible` просто делать `return;`, чтобы не тратить ресурсы GPU/CPU.

### 3.2. Windows: Никогда не использовать `AttachThreadInput`
- **Проблема**: Попытка обойти ограничения `SetForegroundWindow` через `AttachThreadInput` между потоком трея и потоком интерфейса приводит к немедленному дедлоку очередей сообщений Windows, если один из потоков находится в ожидании другого.
- **Решение**: `AttachThreadInput` полностью исключен. Для гарантированной передачи фокуса вызывается `AllowSetForegroundWindow(GetCurrentProcessId())` и стандартный `BringWindowToTop(h)`.

### 3.3. Windows: Хоткей `Alt+Space`
- **Проблема**: Windows Shell намертво резервирует `Alt+Space` под системное меню активного окна (`SC_KEYMENU`). Стандартный `RegisterHotKey(MOD_ALT, VK_SPACE)` либо возвращает ошибку, либо не срабатывает при фокусе в некоторых окнах.
- **Решение**: В [tray.rs](file:///Z:/jlauncher/src/tray.rs) на потоке трея установлен глобальный низкоуровневый хук `WH_KEYBOARD_LL` (`SetWindowsHookExW`). При обнаружении `VK_SPACE` с зажатым `Alt` (`LLKHF_ALTDOWN` / `GetAsyncKeyState`), хук вызывает `coordinator::toggle_window()` и возвращает `1`, «проглатывая» событие клавиши, чтобы системное меню Windows не появлялось. В `coordinator::toggle_window()` встроен debounce 250 мс для защиты от дребезга.

### 3.4. Windows: Всплывающее меню трея (`TrackPopupMenu`)
- **Проблема 1**: Нельзя указывать стиль `WS_EX_NOACTIVATE` для фиктивного окна трея (`hwnd`). Иначе Windows отказывается активировать меню и оно моментально закрывается с кодом `cmd: 0`.
- **Проблема 2**: Меню необходимо создавать динамически через `CreatePopupMenu()`, отображать с флагом `TPM_RETURNCMD`, слать `PostMessageW(hwnd, WM_NULL, 0, 0)` и уничтожать через `DestroyMenu()`. Обрабатывать строго событие `WM_RBUTTONUP` (не `WM_CONTEXTMENU`), чтобы избежать повторного триггера.

### 3.5. Linux: Wayland vs X11
- В X11 глобальный хоткей работает через `global-hotkey` (XGrabKey).
- В Wayland протокол безопасности запрещает приложениям перехватывать глобальные нажатия клавиш.
- **Решение для Wayland**: Пользователь в настройках среды рабочего стола (GNOME Settings $\rightarrow$ Keyboard Shortcuts / KDE Shortcuts) привязывает желаемый хоткей (например, `Super+Space` или `Alt+Space`) к команде:
  ```bash
  jlauncher --toggle
  ```
  Команда подключается по локальному сокету IPC к уже запущенному демону `jlauncher` и мгновенно показывает/скрывает окно.

---

## 4. Сборка, тестирование и запуск

### Требования к окружению:
- Rust toolchain (`cargo`, `rustc`).
- На Windows при сборке через GCC/MinGW убедиться, что `C:\Users\lotarev\.w64devkit\bin` находится в `$env:PATH` (для утилит `dlltool.exe` и линковщика).

### Команды:
```powershell
# Запуск тестов (10 тестов: калькулятор, IPC, конфиг, хоткеи, трей)
$env:PATH = "C:\Users\lotarev\.w64devkit\bin;$env:PATH"
cargo test

# Сборка оптимизированного релизного бинарника
cargo build --release

# Остановка старых процессов и копирование в корень проекта
Stop-Process -Name jlauncher -Force -ErrorAction SilentlyContinue
Copy-Item C:\Users\lotarev\.cargo_target\jlauncher\release\jlauncher.exe z:\jlauncher\jlauncher.exe -Force

# Запуск приложения
z:\jlauncher\jlauncher.exe
```

### Логирование:
Лог-файл пишется в:
`%LOCALAPPDATA%\jlauncher\jlauncher\data\jlauncher.log`
(в кодовой базе инициализируется через `init_logger()` в `src/main.rs`).

---

## 5. Проверенные сценарии работы
1. **Запуск**: Создается трей-иконка, регистрируется HWND окна лаунчера, окно открывается по центру экрана.
2. **Скрытие**: Нажатие `Esc` мгновенно скрывает окно лаунчера.
3. **Показ по хоткею**: Нажатие `Alt+Space` из любого приложения мгновенно восстанавливает окно лаунчера с фокусом в поле ввода.
4. **Показ по клику в трее**: Левый клик по иконке в трее всегда открывает окно лаунчера.
5. **Контекстное меню трея**: Правый клик открывает контекстное меню:
   - «Open Launcher / Показать лаунчер»
   - «Settings / Настройки...»
   - «Rescan Apps / Обновить программы»
   - «Quit / Выход»
6. **Калькулятор**: Ввод выражения (например `2+2`, `sqrt(100)`, `25% of 80`) сразу выдает результат, по `Enter` результат копируется в буфер обмена.
7. **Команды**: `shutdown`, `reboot`, `lock`, `sleep`, `logout` корректно распознаются с локализацией на русском и английском.
