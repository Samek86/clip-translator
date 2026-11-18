use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS, MOD_NOREPEAT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, PeekMessageW, PostThreadMessageW, TranslateMessage, MSG,
    PM_NOREMOVE, WM_APP, WM_HOTKEY,
};

use crate::hotkey_spec::HotkeySpec;

const HOTKEY_ID: i32 = 1;
const WM_APP_COMMAND: u32 = WM_APP + 1;

enum Command {
    Set(HotkeySpec),
    Pause,
    Resume,
    Stop,
}

/// Owns the thread that receives `WM_HOTKEY`. Slint's event loop does not have
/// to dispatch that message.
pub struct HotkeyService {
    thread_id: u32,
    command: Arc<Mutex<Option<Command>>>,
    replies: Receiver<Result<(), String>>,
    presses: Receiver<()>,
    join: Option<JoinHandle<()>>,
}

impl HotkeyService {
    pub fn start(initial: HotkeySpec) -> Result<Self, String> {
        let (id_tx, id_rx) = mpsc::channel();
        let (reply_tx, replies) = mpsc::channel();
        let (press_tx, presses) = mpsc::channel();
        let command = Arc::new(Mutex::new(None));
        let command_thread = Arc::clone(&command);
        let join = std::thread::Builder::new()
            .name("clip-translator-hotkey".into())
            .spawn(move || hotkey_thread(id_tx, reply_tx, press_tx, command_thread, initial))
            .map_err(|err| err.to_string())?;
        let thread_id = id_rx
            .recv_timeout(Duration::from_secs(2))
            .map_err(|_| "Could not start the shortcut thread.".to_string())?;
        let service = Self {
            thread_id,
            command,
            replies,
            presses,
            join: Some(join),
        };
        // The thread registers `initial` itself before sending its id.
        Ok(service)
    }

    pub fn set(&self, spec: HotkeySpec) -> Result<(), String> {
        self.send(Command::Set(spec))
    }

    pub fn pause(&self) {
        let _ = self.send(Command::Pause);
    }

    pub fn resume(&self) {
        let _ = self.send(Command::Resume);
    }

    pub fn drain_presses(&self) -> bool {
        let mut pressed = false;
        while self.presses.try_recv().is_ok() {
            pressed = true;
        }
        pressed
    }

    pub fn stop(&mut self) {
        if self.join.is_none() {
            return;
        }
        let _ = self.send(Command::Stop);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }

    fn send(&self, command: Command) -> Result<(), String> {
        while self.replies.try_recv().is_ok() {}
        *self.command.lock().unwrap_or_else(|err| err.into_inner()) = Some(command);
        unsafe {
            PostThreadMessageW(self.thread_id, WM_APP_COMMAND, WPARAM(0), LPARAM(0))
                .map_err(|err| err.to_string())?;
        }
        self.replies
            .recv_timeout(Duration::from_secs(2))
            .map_err(|_| "The shortcut thread did not respond.".to_string())?
    }
}

impl Drop for HotkeyService {
    fn drop(&mut self) {
        self.stop();
    }
}

fn hotkey_thread(
    id_tx: Sender<u32>,
    reply_tx: Sender<Result<(), String>>,
    press_tx: Sender<()>,
    command: Arc<Mutex<Option<Command>>>,
    initial: HotkeySpec,
) {
    unsafe {
        let mut warmup = MSG::default();
        let _ = PeekMessageW(&mut warmup, None, WM_APP, WM_APP, PM_NOREMOVE);
    }
    let thread_id = unsafe { GetCurrentThreadId() };
    let mut saved = initial;
    let mut registered = register(&saved);
    if !registered {
        super::platform::log("could not register the initial shortcut");
    }
    if id_tx.send(thread_id).is_err() {
        return;
    }

    loop {
        let mut message = MSG::default();
        let status = unsafe { GetMessageW(&mut message, None, 0, 0) };
        if !status.as_bool() {
            break;
        }
        if message.message == WM_HOTKEY && registered {
            let _ = press_tx.send(());
            continue;
        }
        if message.message == WM_APP_COMMAND {
            let next = command.lock().unwrap_or_else(|err| err.into_inner()).take();
            let result = match next {
                Some(Command::Set(spec)) => {
                    unregister();
                    if register(&spec) {
                        saved = spec;
                        registered = true;
                        Ok(())
                    } else {
                        registered = register(&saved);
                        Err(crate::ui_text::HOTKEY_REGISTER_FAILED.to_string())
                    }
                }
                Some(Command::Pause) => {
                    unregister();
                    registered = false;
                    Ok(())
                }
                Some(Command::Resume) => {
                    if !registered {
                        registered = register(&saved);
                    }
                    Ok(())
                }
                Some(Command::Stop) | None => {
                    unregister();
                    let _ = reply_tx.send(Ok(()));
                    break;
                }
            };
            let _ = reply_tx.send(result);
            continue;
        }
        unsafe {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

fn register(spec: &HotkeySpec) -> bool {
    unsafe {
        let modifiers = HOT_KEY_MODIFIERS(spec.modifier_bits());
        if RegisterHotKey(None, HOTKEY_ID, modifiers, spec.key.virtual_key()).is_ok() {
            return true;
        }
        let without_repeat = HOT_KEY_MODIFIERS(spec.modifier_bits() & !MOD_NOREPEAT.0);
        RegisterHotKey(None, HOTKEY_ID, without_repeat, spec.key.virtual_key()).is_ok()
    }
}

fn unregister() {
    unsafe {
        let _ = UnregisterHotKey(None, HOTKEY_ID);
    }
}
