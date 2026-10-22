//! The screen of the Mac for the push to a phone.
//!
//! A locked screen or a sleeping display counts as no focus: the person left. The app reads the
//! distributed notifications `com.apple.screenIsLocked` and `com.apple.screenIsUnlocked`, and the
//! notifications of `NSWorkspace` when the displays sleep and wake **[verify]**. It tells the core
//! each change with `CoreHandle::report_screen`. The facts come as events, so no timer runs. On
//! other systems the app reports nothing, and the core uses the focus alone.

use nh_core::CoreHandle;

/// Follows the screen for the core in this process, until the app ends.
#[cfg(target_os = "macos")]
pub fn follow(core: CoreHandle) {
    use std::{
        ptr::NonNull,
        sync::{Arc, Mutex, PoisonError},
    };

    use block2::RcBlock;
    use objc2_app_kit::{
        NSWorkspace, NSWorkspaceScreensDidSleepNotification, NSWorkspaceScreensDidWakeNotification,
    };
    use objc2_foundation::{NSDistributedNotificationCenter, NSNotification, NSString};

    /// What the app knows of the screen: locked, and the display asleep.
    type Facts = Arc<Mutex<(bool, bool)>>;

    let facts: Facts = Arc::new(Mutex::new((false, false)));
    // Each observer changes one fact and reports both.
    let observer = |change: fn(&mut (bool, bool))| {
        let facts = Arc::clone(&facts);
        let core = core.clone();
        RcBlock::new(move |_: NonNull<NSNotification>| {
            let (locked, asleep) = {
                let mut now = facts.lock().unwrap_or_else(PoisonError::into_inner);
                change(&mut now);
                *now
            };
            tracing::debug!(locked, asleep, "the screen changed");
            core.report_screen(locked, asleep);
        })
    };
    let lock = observer(|facts| facts.0 = true);
    let unlock = observer(|facts| facts.0 = false);
    let sleep = observer(|facts| facts.1 = true);
    let wake = observer(|facts| facts.1 = false);

    let distributed = NSDistributedNotificationCenter::defaultCenter();
    let workspace = NSWorkspace::sharedWorkspace().notificationCenter();
    let locked = NSString::from_str("com.apple.screenIsLocked");
    let unlocked = NSString::from_str("com.apple.screenIsUnlocked");
    // SAFETY: the names are notification names, no object filters them, the blocks are `Send`
    // (they hold a core handle and a mutex), and a `None` queue runs them on the thread that posts.
    // The centres keep the observers for the life of the app, which never removes them.
    let tokens = unsafe {
        [
            distributed.addObserverForName_object_queue_usingBlock(
                Some(&locked),
                None,
                None,
                &lock,
            ),
            distributed.addObserverForName_object_queue_usingBlock(
                Some(&unlocked),
                None,
                None,
                &unlock,
            ),
            workspace.addObserverForName_object_queue_usingBlock(
                Some(NSWorkspaceScreensDidSleepNotification),
                None,
                None,
                &sleep,
            ),
            workspace.addObserverForName_object_queue_usingBlock(
                Some(NSWorkspaceScreensDidWakeNotification),
                None,
                None,
                &wake,
            ),
        ]
    };
    // The observers stay for the life of the app.
    std::mem::forget(tokens);
}

/// Other systems report nothing: the core uses the focus alone.
#[cfg(not(target_os = "macos"))]
pub fn follow(_core: CoreHandle) {}
