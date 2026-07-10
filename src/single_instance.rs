//! Single-instance guard: a named mutex detects a running instance; a named event
//! lets a second launch ask the first to show its window. Windows-only.
#![cfg(windows)]

use std::sync::mpsc::Sender;
use windows_sys::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE};
use windows_sys::Win32::System::Threading::{
    CreateEventW, CreateMutexW, OpenEventW, SetEvent, WaitForSingleObject, INFINITE,
};

/// A UTF-16, NUL-terminated copy of `s` for the `*W` Win32 APIs.
fn wide_vec(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub enum Acquired {
    /// We are the only instance. Hold this until exit.
    First(Guard),
    /// Another instance is already running.
    Already,
}

pub struct Guard {
    mutex: HANDLE,
}

impl Drop for Guard {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.mutex); }
    }
}

/// Try to become the single instance.
pub fn acquire() -> Acquired {
    let name = wide_vec("Local\\g13-driver-singleton");
    let mutex = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
    if mutex.is_null() {
        // Can't create the mutex — fail open (allow launch) rather than block the app.
        return Acquired::First(Guard { mutex: std::ptr::null_mut() });
    }
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        unsafe { CloseHandle(mutex); }
        return Acquired::Already;
    }
    Acquired::First(Guard { mutex })
}

/// Ask the running instance to show its window.
pub fn signal_existing() {
    let name = wide_vec("Local\\g13-driver-activate");
    // 0x0002 = EVENT_MODIFY_STATE
    let ev = unsafe { OpenEventW(0x0002, 0, name.as_ptr()) };
    if !ev.is_null() {
        unsafe { SetEvent(ev); CloseHandle(ev); }
    }
}

/// Spawn a waiter that fires `on_activate` whenever a later launch pings the event.
/// Call once, from the first instance, after the window exists.
pub fn spawn_activation_waiter(on_activate: Sender<()>) {
    let name = wide_vec("Local\\g13-driver-activate");
    let ev = unsafe { CreateEventW(std::ptr::null(), 0, 0, name.as_ptr()) };
    if ev.is_null() { return; }
    // Transmute HANDLE (*mut c_void) to usize so the closure is Send.
    // SAFETY: the event handle is valid and used read-only for waiting.
    let ev_bits = ev as usize;
    std::thread::spawn(move || loop {
        let handle = ev_bits as HANDLE;
        let r = unsafe { WaitForSingleObject(handle, INFINITE) };
        if r != 0 { break; } // WAIT_OBJECT_0 == 0; anything else -> stop
        if on_activate.send(()).is_err() { break; }
    });
}
