//! One tray icon for the default output device, and an optional one for the microphone.
//!
//! The wheel changes volume, the middle button toggles mute, left click opens the window and
//! right click lists the devices. Global hotkeys are registered on this same hidden window.

mod icon;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::mem::size_of;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::Mutex;
use std::thread;

use audioctl_core::command::Value;
use audioctl_core::config::resolve_icon_file;
use audioctl_core::hotkey::{self, MOD_NOREPEAT};
use audioctl_core::icon::IconRequest;
use audioctl_core::{DeviceSnapshot, Engine, Event, Flow};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS};
use windows::Win32::UI::Shell::{
    Shell_NotifyIconGetRect, Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIM_ADD, NIM_DELETE,
    NIM_MODIFY, NIM_SETVERSION, NOTIFYICONDATAW, NOTIFYICONIDENTIFIER, NOTIFYICON_VERSION_4,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CallNextHookEx, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyIcon, DestroyMenu,
    DispatchMessageW, GetCursorPos, GetMessageW, PostMessageW, PostQuitMessage, RegisterClassExW, RegisterWindowMessageW,
    SendMessageTimeoutW, SetForegroundWindow, SetWindowsHookExW, TrackPopupMenuEx, TranslateMessage, HICON, HMENU,
    MF_CHECKED, MF_POPUP, MF_SEPARATOR, MF_STRING, MSG, MSLLHOOKSTRUCT, SMTO_ABORTIFHUNG, TPM_BOTTOMALIGN, TPM_RETURNCMD,
    TPM_RIGHTBUTTON, WH_MOUSE_LL, WINDOW_EX_STYLE, WM_APP, WM_CONTEXTMENU, WM_DPICHANGED, WM_HOTKEY, WM_LBUTTONUP,
    WM_MBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WNDCLASSEXW, WS_OVERLAPPED,
};

pub struct Hooks {
    pub open: Box<dyn Fn() + Send>,
    pub quit: Box<dyn Fn() + Send>,
}

const WM_TRAY: u32 = WM_APP + 1;
const WM_REFRESH: u32 = WM_APP + 2;
const WM_WHEEL: u32 = WM_APP + 3;
const WM_SHUTDOWN: u32 = WM_APP + 4;

const UID_OUTPUT: u32 = 1;
const UID_INPUT: u32 = 2;

const CMD_OPEN: usize = 1;
const CMD_RESCAN: usize = 2;
const CMD_QUIT: usize = 3;
const CMD_OUT: usize = 100;
const CMD_IN: usize = 200;
const CMD_MENU: usize = 300;

static HWND_RAW: AtomicIsize = AtomicIsize::new(0);
static REFRESH_PENDING: AtomicBool = AtomicBool::new(false);
static ICON_RECTS: Mutex<Vec<(RECT, u32)>> = Mutex::new(Vec::new());

struct Entry {
    device_id: String,
    signature: String,
    hicon: HICON,
}

struct Hotkey {
    id: i32,
    command: String,
}

struct State {
    engine: Engine,
    hooks: Hooks,
    hwnd: HWND,
    entries: HashMap<u32, Entry>,
    hotkeys: Vec<Hotkey>,
    warned: HashSet<String>,
    taskbar_created: u32,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

pub fn spawn(engine: Engine, hooks: Hooks) {
    thread::Builder::new()
        .name("tray".into())
        .spawn(move || {
            if let Err(e) = run(engine, hooks) {
                tracing::error!("tray: {e:#}");
            }
        })
        .expect("spawn tray thread");
}

pub fn shutdown() {
    let hwnd = HWND(HWND_RAW.load(Ordering::SeqCst) as *mut _);
    if hwnd.0.is_null() {
        return;
    }
    unsafe {
        let _ = SendMessageTimeoutW(hwnd, WM_SHUTDOWN, WPARAM(0), LPARAM(0), SMTO_ABORTIFHUNG, 2000, None);
    }
}

fn run(engine: Engine, hooks: Hooks) -> anyhow::Result<()> {
    unsafe {
        let hinstance = GetModuleHandleW(None)?;
        let class = w!("audioctl_tray");
        let wc = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wndproc),
            hInstance: hinstance.into(),
            lpszClassName: class,
            ..Default::default()
        };
        RegisterClassExW(&wc);
        let hwnd = CreateWindowExW(WINDOW_EX_STYLE(0), class, w!("audioctl"), WS_OVERLAPPED, 0, 0, 0, 0, None, None, Some(hinstance.into()), None)?;
        HWND_RAW.store(hwnd.0 as isize, Ordering::SeqCst);
        let taskbar_created = RegisterWindowMessageW(w!("TaskbarCreated"));
        STATE.with(|s| {
            *s.borrow_mut() = Some(State { engine: engine.clone(), hooks, hwnd, entries: HashMap::new(), hotkeys: Vec::new(), warned: HashSet::new(), taskbar_created });
        });

        let mut rx = engine.subscribe();
        thread::Builder::new().name("tray-events".into()).spawn(move || loop {
            match rx.blocking_recv() {
                Ok(Event::Quit) | Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                Ok(_) | Err(_) => request_refresh(),
            }
        })?;

        let _hook = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook), Some(hinstance.into()), 0)?;
        request_refresh();
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0) != false {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    Ok(())
}

fn request_refresh() {
    if !REFRESH_PENDING.swap(true, Ordering::SeqCst) {
        let hwnd = HWND(HWND_RAW.load(Ordering::SeqCst) as *mut _);
        unsafe {
            let _ = PostMessageW(Some(hwnd), WM_REFRESH, WPARAM(0), LPARAM(0));
        }
    }
}

unsafe extern "system" fn mouse_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 && wparam.0 as u32 == WM_MOUSEWHEEL {
        let info = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
        let pt = info.pt;
        let hit = ICON_RECTS.lock().ok().and_then(|rects| {
            rects.iter().find(|(r, _)| pt.x >= r.left && pt.x < r.right && pt.y >= r.top && pt.y < r.bottom).map(|(_, uid)| *uid)
        });
        if let Some(uid) = hit {
            let delta = (info.mouseData >> 16) as i16;
            let hwnd = HWND(HWND_RAW.load(Ordering::SeqCst) as *mut _);
            let _ = PostMessageW(Some(hwnd), WM_WHEEL, WPARAM(uid as usize), LPARAM(delta as isize));
            return LRESULT(1);
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let handled = STATE.with(|s| {
        let mut guard = match s.try_borrow_mut() {
            Ok(g) => g,
            Err(_) => {
                if msg == WM_REFRESH {
                    REFRESH_PENDING.store(false, Ordering::SeqCst);
                }
                return false;
            }
        };
        let Some(state) = guard.as_mut() else { return false };
        match msg {
            WM_REFRESH => {
                REFRESH_PENDING.store(false, Ordering::SeqCst);
                state.sync();
                true
            }
            WM_WHEEL => {
                state.on_wheel(wparam.0 as u32, lparam.0 as i16);
                true
            }
            WM_HOTKEY => {
                state.on_hotkey(wparam.0 as i32);
                true
            }
            WM_TRAY => {
                let event = (lparam.0 & 0xFFFF) as u32;
                let uid = ((lparam.0 >> 16) & 0xFFFF) as u32;
                match event {
                    WM_MOUSEMOVE => state.update_rects(),
                    WM_LBUTTONUP => (state.hooks.open)(),
                    WM_MBUTTONUP => state.on_middle(uid),
                    WM_CONTEXTMENU => {
                        let action = state.track_menu();
                        drop(guard);
                        run_action(action);
                        request_refresh();
                        return true;
                    }
                    _ => {}
                }
                true
            }
            WM_DPICHANGED => {
                state.sync();
                false
            }
            WM_SHUTDOWN => {
                state.clear_hotkeys();
                state.remove_all();
                PostQuitMessage(0);
                true
            }
            m if m == state.taskbar_created && m != 0 => {
                for (_, entry) in state.entries.drain() {
                    let _ = DestroyIcon(entry.hicon);
                }
                state.sync();
                true
            }
            _ => false,
        }
    });
    if handled { LRESULT(0) } else { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

enum Action {
    Open,
    Quit,
    Rescan,
    Command(String),
}

fn run_action(action: Option<Action>) {
    let Some(action) = action else { return };
    match action {
        Action::Open => STATE.with(|s| {
            if let Some(st) = s.borrow().as_ref() {
                (st.hooks.open)();
            }
        }),
        Action::Quit => STATE.with(|s| {
            if let Some(st) = s.borrow().as_ref() {
                (st.hooks.quit)();
            }
        }),
        Action::Rescan => STATE.with(|s| {
            if let Some(st) = s.borrow().as_ref() {
                let engine = st.engine.clone();
                thread::spawn(move || {
                    let _ = engine.rescan();
                });
            }
        }),
        Action::Command(command) => STATE.with(|s| {
            if let Some(st) = s.borrow().as_ref() {
                let engine = st.engine.clone();
                thread::spawn(move || {
                    if let Err(e) = engine.execute(&command) {
                        tracing::warn!("menu '{command}': {e:#}");
                    }
                });
            }
        }),
    }
}

impl State {
    fn nid(&self, uid: u32) -> NOTIFYICONDATAW {
        NOTIFYICONDATAW { cbSize: size_of::<NOTIFYICONDATAW>() as u32, hWnd: self.hwnd, uID: uid, uCallbackMessage: WM_TRAY, ..Default::default() }
    }

    fn sync(&mut self) {
        let cfg = self.engine.config();
        self.sync_hotkeys(&cfg);
        if !cfg.tray.enabled {
            self.remove_all();
            return;
        }
        let devices = self.engine.devices();
        let mut wanted = Vec::new();
        if let Some(device) = devices.iter().find(|d| d.flow == Flow::Render && d.is_default) {
            wanted.push((UID_OUTPUT, device));
        }
        if cfg.tray.show_input_icon {
            if let Some(device) = devices.iter().find(|d| d.flow == Flow::Capture && d.is_default) {
                wanted.push((UID_INPUT, device));
            }
        }
        let keep: Vec<u32> = wanted.iter().map(|(uid, _)| *uid).collect();
        for uid in self.entries.keys().copied().filter(|uid| !keep.contains(uid)).collect::<Vec<_>>() {
            self.remove(uid);
        }
        let size = icon::tray_px().max(16) as u32;
        let badge = cfg.tray.show_volume_badge;
        for (uid, device) in wanted {
            let signature = format!("{}:{}:{}:{}:{}:{size}:{badge}", device.id, device.volume, device.muted, device.icon_glyph, device.icon_color);
            if self.entries.get(&uid).is_some_and(|e| e.signature == signature) {
                continue;
            }
            let Some(hicon) = icon::render(&icon_request(device, size, badge)) else { continue };
            self.install(uid, device, signature, hicon);
        }
        self.update_rects();
    }

    fn install(&mut self, uid: u32, device: &DeviceSnapshot, signature: String, hicon: HICON) {
        let mut nid = self.nid(uid);
        nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP | NIF_SHOWTIP;
        nid.hIcon = hicon;
        let tip = if device.muted {
            format!("{}\nmuted", device.display_name)
        } else {
            format!("{}\n{}%", device.display_name, device.volume)
        };
        wide_into(&mut nid.szTip, &tip);
        unsafe {
            if let Some(old) = self.entries.get(&uid) {
                let _ = Shell_NotifyIconW(NIM_MODIFY, &nid);
                let _ = DestroyIcon(old.hicon);
            } else {
                let _ = Shell_NotifyIconW(NIM_ADD, &nid);
                nid.Anonymous.uVersion = NOTIFYICON_VERSION_4;
                let _ = Shell_NotifyIconW(NIM_SETVERSION, &nid);
            }
        }
        self.entries.insert(uid, Entry { device_id: device.id.clone(), signature, hicon });
    }

    fn sync_hotkeys(&mut self, cfg: &audioctl_core::Config) {
        self.clear_hotkeys();
        for (index, hotkey) in cfg.hotkeys.iter().enumerate() {
            let Ok(combo) = hotkey::parse(&hotkey.keys) else {
                if self.warned.insert(hotkey.keys.clone()) {
                    self.engine.notify(format!("Hotkey '{}' is not a valid combination", hotkey.keys));
                }
                continue;
            };
            let id = (index + 1) as i32;
            let modifiers = HOT_KEY_MODIFIERS(combo.modifiers | MOD_NOREPEAT);
            let ok = unsafe { RegisterHotKey(Some(self.hwnd), id, modifiers, combo.vk).is_ok() };
            if ok {
                self.warned.remove(&hotkey.keys);
                self.hotkeys.push(Hotkey { id, command: hotkey.command.clone() });
            } else if self.warned.insert(hotkey.keys.clone()) {
                self.engine.notify(format!("Hotkey {} is already used by another program", hotkey.keys));
            }
        }
    }

    fn clear_hotkeys(&mut self) {
        for hotkey in self.hotkeys.drain(..) {
            unsafe {
                let _ = UnregisterHotKey(Some(self.hwnd), hotkey.id);
            }
        }
    }

    fn on_hotkey(&self, id: i32) {
        let Some(command) = self.hotkeys.iter().find(|h| h.id == id).map(|h| h.command.clone()) else { return };
        let engine = self.engine.clone();
        thread::spawn(move || {
            if let Err(e) = engine.execute(&command) {
                tracing::warn!("hotkey '{command}': {e:#}");
                engine.notify(format!("{command}: {e:#}"));
            }
        });
    }

    fn on_wheel(&self, uid: u32, delta: i16) {
        let Some(entry) = self.entries.get(&uid) else { return };
        let step = self.engine.config().tray.wheel_step.max(1) as i32;
        let notches = (delta as i32 / 120).clamp(-10, 10);
        let notches = if notches == 0 { delta.signum() as i32 } else { notches };
        let id = entry.device_id.clone();
        let _ = self.engine.adjust_volume(&id, Value::Rel(step * notches));
    }

    fn on_middle(&self, uid: u32) {
        let Some(entry) = self.entries.get(&uid) else { return };
        let _ = self.engine.adjust_mute(&entry.device_id);
    }

    fn track_menu(&self) -> Option<Action> {
        let devices = self.engine.devices();
        let cfg = self.engine.config();
        unsafe {
            let menu = CreatePopupMenu().ok()?;
            // AppendMenu copies the label, so the temporary wide string only has to live for the call.
            let add = |menu: HMENU, flags, id: usize, label: &str| {
                let wide: Vec<u16> = label.encode_utf16().chain(std::iter::once(0)).collect();
                let _ = AppendMenuW(menu, flags, id, PCWSTR(wide.as_ptr()));
            };
            let outputs: Vec<&DeviceSnapshot> = devices.iter().filter(|d| d.flow == Flow::Render && !d.hidden).collect();
            for (i, device) in outputs.iter().enumerate() {
                let flags = if device.is_default { MF_STRING | MF_CHECKED } else { MF_STRING };
                add(menu, flags, CMD_OUT + i, &device.display_name);
            }
            let inputs: Vec<&DeviceSnapshot> = devices.iter().filter(|d| d.flow == Flow::Capture && !d.hidden).collect();
            if !inputs.is_empty() {
                if let Ok(sub) = CreatePopupMenu() {
                    for (i, device) in inputs.iter().enumerate() {
                        let flags = if device.is_default { MF_STRING | MF_CHECKED } else { MF_STRING };
                        add(sub, flags, CMD_IN + i, &device.display_name);
                    }
                    let wide: Vec<u16> = "Microphones".encode_utf16().chain(std::iter::once(0)).collect();
                    let _ = AppendMenuW(menu, MF_STRING | MF_POPUP, sub.0 as usize, PCWSTR(wide.as_ptr()));
                }
            }
            if !cfg.menu.is_empty() {
                let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
                for (i, entry) in cfg.menu.iter().enumerate() {
                    add(menu, MF_STRING, CMD_MENU + i, &entry.label);
                }
            }
            let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
            add(menu, MF_STRING, CMD_OPEN, "Open");
            add(menu, MF_STRING, CMD_RESCAN, "Rescan");
            add(menu, MF_STRING, CMD_QUIT, "Quit");

            let mut point = POINT::default();
            let _ = GetCursorPos(&mut point);
            let _ = SetForegroundWindow(self.hwnd);
            let picked = TrackPopupMenuEx(menu, (TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN).0, point.x, point.y, self.hwnd, None).0 as usize;
            let _ = DestroyMenu(menu);
            let action = if (CMD_OUT..CMD_IN).contains(&picked) {
                outputs.get(picked - CMD_OUT).map(|d| Action::Command(format!("o \"{}\"", d.id)))
            } else if (CMD_IN..CMD_MENU).contains(&picked) {
                inputs.get(picked - CMD_IN).map(|d| Action::Command(format!("i \"{}\"", d.id)))
            } else if (CMD_MENU..CMD_MENU + cfg.menu.len()).contains(&picked) {
                Some(Action::Command(cfg.menu[picked - CMD_MENU].command.clone()))
            } else {
                match picked {
                    CMD_OPEN => Some(Action::Open),
                    CMD_RESCAN => Some(Action::Rescan),
                    CMD_QUIT => Some(Action::Quit),
                    _ => None,
                }
            };
            action
        }
    }

    fn remove(&mut self, uid: u32) {
        if let Some(entry) = self.entries.remove(&uid) {
            unsafe {
                let _ = Shell_NotifyIconW(NIM_DELETE, &self.nid(uid));
                let _ = DestroyIcon(entry.hicon);
            }
        }
    }

    fn remove_all(&mut self) {
        for uid in self.entries.keys().copied().collect::<Vec<_>>() {
            self.remove(uid);
        }
        self.update_rects();
    }

    fn update_rects(&self) {
        let mut rects = Vec::new();
        for &uid in self.entries.keys() {
            let id = NOTIFYICONIDENTIFIER { cbSize: size_of::<NOTIFYICONIDENTIFIER>() as u32, hWnd: self.hwnd, uID: uid, ..Default::default() };
            if let Ok(rect) = unsafe { Shell_NotifyIconGetRect(&id) } {
                rects.push((rect, uid));
            }
        }
        if let Ok(mut guard) = ICON_RECTS.lock() {
            *guard = rects;
        }
    }
}

fn icon_request(device: &DeviceSnapshot, size: u32, badge: bool) -> IconRequest {
    IconRequest {
        glyph: device.icon_char.chars().next().unwrap_or('\u{E7F5}'),
        color: audioctl_core::icon::parse_color(&device.icon_color),
        file: device.icon_file.as_deref().map(resolve_icon_file),
        muted: device.muted,
        badge: badge.then_some(device.volume),
        size,
    }
}

fn wide_into<const N: usize>(dst: &mut [u16; N], text: &str) {
    let mut i = 0;
    for unit in text.encode_utf16().take(N - 1) {
        dst[i] = unit;
        i += 1;
    }
    dst[i] = 0;
}
