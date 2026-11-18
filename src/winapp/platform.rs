use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::sync::OnceLock;

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::core::w;
use windows::Win32::Foundation::{
    CloseHandle, LocalFree, HANDLE, HLOCAL, HWND, POINT, RECT, WAIT_ABANDONED, WAIT_OBJECT_0,
    WAIT_TIMEOUT,
};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::Security::Cryptography::{
    CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
};
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::Win32::System::Threading::{
    AttachThreadInput, CreateEventW, CreateMutexW, GetCurrentThreadId, OpenEventW, SetEvent,
    WaitForSingleObject, EVENT_MODIFY_STATE, INFINITE,
};
use windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetForegroundWindow, GetWindowLongPtrW, GetWindowThreadProcessId, MessageBoxW,
    SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, ShowWindow, GWL_EXSTYLE, GWL_STYLE,
    HWND_TOP, MB_ICONERROR, MB_OK, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    SWP_NOZORDER, SW_HIDE, SW_SHOW, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW, WS_MAXIMIZEBOX,
    WS_MINIMIZEBOX,
};
use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE};
use winreg::RegKey;

use crate::secrets::Protector;

static LOG_PATH: OnceLock<PathBuf> = OnceLock::new();
static INSTANCE_MUTEX: OnceLock<isize> = OnceLock::new();
static ACTIVATE_EVENT: OnceLock<isize> = OnceLock::new();

const ENTROPY: &[u8] = b"clip-translator-v1";

pub fn app_dir() -> PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        if !appdata.is_empty() {
            return PathBuf::from(appdata).join("clip-translator");
        }
    }
    PathBuf::from(".").join("clip-translator")
}

pub fn init_log(dir: &std::path::Path) {
    let _ = fs::create_dir_all(dir);
    let path = dir.join("app.log");
    if let Ok(meta) = fs::metadata(&path) {
        if meta.len() > 1_000_000 {
            let _ = fs::write(&path, b"");
        }
    }
    let _ = LOG_PATH.set(path);
    log("시작");
}

pub fn log(message: &str) {
    let Some(path) = LOG_PATH.get() else {
        return;
    };
    let line = format!("{} {message}\n", local_stamp());
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = file.write_all(line.as_bytes());
    }
}

pub fn error_box(text: &str) {
    unsafe {
        let _ = MessageBoxW(
            None,
            &windows::core::HSTRING::from(text),
            &windows::core::HSTRING::from("클립 번역"),
            MB_OK | MB_ICONERROR,
        );
    }
}

pub fn set_app_id() {
    unsafe {
        let _ = SetCurrentProcessExplicitAppUserModelID(w!("Jun.clip-translator"));
    }
}

/// Returns `Ok(true)` for the owning instance. `Ok(false)` means another
/// instance is already running and has been asked to show settings.
pub fn claim_single_instance() -> Result<bool, String> {
    unsafe {
        let mutex = CreateMutexW(None, true, w!("Local\\clip-translator"))
            .map_err(|err| err.to_string())?;
        let wait = WaitForSingleObject(mutex, 0);
        if wait == WAIT_TIMEOUT {
            signal_existing_instance();
            let _ = CloseHandle(mutex);
            return Ok(false);
        }
        if wait != WAIT_OBJECT_0 && wait != WAIT_ABANDONED {
            let _ = CloseHandle(mutex);
            return Err(format!("단일 인스턴스 확인에 실패했습니다 ({wait:?})"));
        }
        let event = CreateEventW(None, false, false, w!("Local\\clip-translator-activate"))
            .map_err(|err| err.to_string())?;
        let _ = INSTANCE_MUTEX.set(handle_bits(mutex));
        let _ = ACTIVATE_EVENT.set(handle_bits(event));
        Ok(true)
    }
}

pub fn spawn_activation_watcher(tx: Sender<()>) {
    let Some(bits) = ACTIVATE_EVENT.get().copied() else {
        return;
    };
    std::thread::spawn(move || {
        let event = handle_from_bits(bits);
        loop {
            let wait = unsafe { WaitForSingleObject(event, INFINITE) };
            if wait == WAIT_OBJECT_0 {
                if tx.send(()).is_err() {
                    break;
                }
            } else {
                break;
            }
        }
    });
}

fn signal_existing_instance() {
    unsafe {
        if let Ok(event) = OpenEventW(
            EVENT_MODIFY_STATE,
            false,
            w!("Local\\clip-translator-activate"),
        ) {
            let _ = SetEvent(event);
            let _ = CloseHandle(event);
        }
    }
}

pub fn local_year_month() -> String {
    let time = unsafe { GetLocalTime() };
    format!("{:04}-{:02}", time.wYear, time.wMonth)
}

pub fn local_stamp() -> String {
    let time = unsafe { GetLocalTime() };
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        time.wYear, time.wMonth, time.wDay, time.wHour, time.wMinute, time.wSecond
    )
}

pub fn system_is_dark() -> bool {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let Ok(key) = hkcu.open_subkey_with_flags(
        "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize",
        KEY_READ,
    ) else {
        return false;
    };
    let light: u32 = key.get_value("AppsUseLightTheme").unwrap_or(1);
    light == 0
}

pub fn set_autostart(enabled: bool) -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _) = hkcu
        .create_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Run")
        .map_err(|err| err.to_string())?;
    if enabled {
        let exe = std::env::current_exe().map_err(|err| err.to_string())?;
        let quoted = quote_command(&exe);
        key.set_value("clip-translator", &quoted)
            .map_err(|err| err.to_string())?;
    } else if let Err(err) = key.delete_value("clip-translator") {
        if err.kind() != std::io::ErrorKind::NotFound {
            return Err(err.to_string());
        }
    }
    let _ = KEY_SET_VALUE;
    Ok(())
}

fn quote_command(path: &std::path::Path) -> String {
    let text = path.to_string_lossy();
    if text.contains(' ') {
        format!("\"{text}\"")
    } else {
        text.into_owned()
    }
}

pub struct Dpapi;

impl Protector for Dpapi {
    fn protect(&self, plain: &[u8]) -> Result<Vec<u8>, String> {
        protect_bytes(plain)
    }

    fn unprotect(&self, blob: &[u8]) -> Result<Vec<u8>, String> {
        unprotect_bytes(blob)
    }
}

fn protect_bytes(plain: &[u8]) -> Result<Vec<u8>, String> {
    unsafe {
        let input = blob(plain);
        let entropy = blob(ENTROPY);
        let mut output = CRYPT_INTEGER_BLOB::default();
        CryptProtectData(
            &input,
            None,
            Some(&entropy),
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
        .map_err(|err| err.to_string())?;
        copy_and_free(output)
    }
}

fn unprotect_bytes(protected: &[u8]) -> Result<Vec<u8>, String> {
    unsafe {
        let input = blob(protected);
        let entropy = blob(ENTROPY);
        let mut output = CRYPT_INTEGER_BLOB::default();
        CryptUnprotectData(
            &input,
            None,
            Some(&entropy),
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
        .map_err(|err| err.to_string())?;
        copy_and_free(output)
    }
}

unsafe fn copy_and_free(output: CRYPT_INTEGER_BLOB) -> Result<Vec<u8>, String> {
    if output.pbData.is_null() {
        return Ok(Vec::new());
    }
    let bytes = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
    let _ = LocalFree(Some(HLOCAL(output.pbData as *mut _)));
    Ok(bytes)
}

fn blob(bytes: &[u8]) -> CRYPT_INTEGER_BLOB {
    CRYPT_INTEGER_BLOB {
        cbData: bytes.len() as u32,
        pbData: bytes.as_ptr() as *mut u8,
    }
}

#[derive(Clone, Copy)]
pub struct WorkArea {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

pub fn cursor_work_area() -> Option<WorkArea> {
    unsafe {
        let mut point = POINT::default();
        GetCursorPos(&mut point).ok()?;
        let monitor = MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST);
        monitor_work_area(monitor)
    }
}

pub enum FrameStyle {
    Settings,
    Translate,
}

pub fn tune_frame(window: &slint::Window, style: FrameStyle) {
    let Some(hwnd) = hwnd_of(window) else {
        return;
    };
    unsafe {
        let mut ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        ex &= !(WS_EX_APPWINDOW.0 as isize);
        ex |= WS_EX_TOOLWINDOW.0 as isize;
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex);

        let mut frame = GetWindowLongPtrW(hwnd, GWL_STYLE);
        frame &= !(WS_MAXIMIZEBOX.0 as isize);
        if matches!(style, FrameStyle::Translate) {
            frame &= !(WS_MINIMIZEBOX.0 as isize);
        }
        SetWindowLongPtrW(hwnd, GWL_STYLE, frame);
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOP),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED | SWP_NOACTIVATE,
        );
        let _ = ShowWindow(hwnd, SW_HIDE);
        let _ = ShowWindow(hwnd, SW_SHOW);
    }
}

pub fn center_on_cursor(window: &slint::Window) {
    let Some(area) = cursor_work_area() else {
        return;
    };
    let size = window.size();
    let width = size.width as i32;
    let height = size.height as i32;
    let mut x = area.x + (area.width - width) / 2;
    let mut y = area.y + (area.height - height) / 2;
    if x < area.x {
        x = area.x;
    }
    if y < area.y {
        y = area.y;
    }
    if width < area.width {
        let max_x = area.x + area.width - width;
        if x > max_x {
            x = max_x;
        }
    }
    if height < area.height {
        let max_y = area.y + area.height - height;
        if y > max_y {
            y = max_y;
        }
    }
    window.set_position(slint::PhysicalPosition::new(x, y));
}

pub fn focus_window(window: &slint::Window) {
    let Some(hwnd) = hwnd_of(window) else {
        return;
    };
    unsafe {
        let foreground = GetForegroundWindow();
        let foreground_thread = GetWindowThreadProcessId(foreground, None);
        let ours = GetCurrentThreadId();
        let attached = foreground_thread != 0
            && foreground_thread != ours
            && AttachThreadInput(foreground_thread, ours, true).as_bool();
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
        if attached {
            let _ = AttachThreadInput(foreground_thread, ours, false);
        }
    }
}

fn hwnd_of(window: &slint::Window) -> Option<HWND> {
    let wrapper = window.window_handle();
    let handle = wrapper.window_handle().ok()?;
    match handle.as_raw() {
        RawWindowHandle::Win32(raw) => Some(HWND(raw.hwnd.get() as *mut std::ffi::c_void)),
        _ => None,
    }
}

fn monitor_work_area(monitor: windows::Win32::Graphics::Gdi::HMONITOR) -> Option<WorkArea> {
    unsafe {
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..MONITORINFO::default()
        };
        if !GetMonitorInfoW(monitor, &mut info).as_bool() {
            return None;
        }
        Some(rect_area(info.rcWork))
    }
}

fn rect_area(rect: RECT) -> WorkArea {
    WorkArea {
        x: rect.left,
        y: rect.top,
        width: rect.right - rect.left,
        height: rect.bottom - rect.top,
    }
}

fn handle_bits(handle: HANDLE) -> isize {
    handle.0 as isize
}

fn handle_from_bits(bits: isize) -> HANDLE {
    HANDLE(bits as *mut std::ffi::c_void)
}
