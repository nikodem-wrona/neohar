//! Interrupt handling. Ctrl-C, SIGTERM and SIGHUP set a flag, and the recorder checks it between
//! reads. The recorder then stops its holder and its child on the normal error path, as for any
//! other failure. Without a handler, the signal would end the process before its guard ran, and
//! a holder would stay behind.

#![allow(unsafe_code)]

use std::sync::atomic::{AtomicBool, Ordering};

static INTERRUPTED: AtomicBool = AtomicBool::new(false);

extern "C" fn on_signal(_signal: libc::c_int) {
    // An atomic store is async-signal-safe.
    INTERRUPTED.store(true, Ordering::SeqCst);
}

/// Makes SIGINT, SIGTERM and SIGHUP set the flag that [`interrupted`] reads.
pub fn install() {
    // SAFETY: the `sigaction` is fully initialised before it is used, and the handler only does
    // an atomic store, which is async-signal-safe.
    unsafe {
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = on_signal as *const () as libc::sighandler_t;
        action.sa_flags = libc::SA_RESTART;
        libc::sigemptyset(&mut action.sa_mask);
        for signal in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP] {
            libc::sigaction(signal, &action, std::ptr::null_mut());
        }
    }
}

/// Whether the process got a signal that asks it to stop.
pub fn interrupted() -> bool {
    INTERRUPTED.load(Ordering::SeqCst)
}
