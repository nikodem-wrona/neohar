//! The rules of the host router, with a host in the process and two link clients that write down
//! what they get.

use std::{path::PathBuf, sync::Arc};

use nh_proto::api::{
    common::Urgency,
    ids::{InteractionId, NotificationId, WindowId},
};
use ulid::Ulid;

use super::*;
use crate::{
    host::{HostCall, NotificationAction, RecordingHost, TrayEntry},
    layout::model::{Window, WindowFacts},
};

const LOCAL: Ulid = Ulid(1);
const LAPTOP: Ulid = Ulid(2);
const STUDIO: Ulid = Ulid(3);

fn info(device: Ulid, name: &str) -> ClientInfo {
    ClientInfo {
        device,
        name: name.to_owned(),
    }
}

fn recording(actions: bool) -> Arc<RecordingHost> {
    Arc::new(RecordingHost::new(PathBuf::from("/app/nh"), actions))
}

/// A router with a host in the process (with buttons) and two link clients: the laptop has
/// buttons, the studio has none.
struct Setup {
    router: HostRouter,
    local: Arc<RecordingHost>,
    laptop: Arc<RecordingHost>,
    studio: Arc<RecordingHost>,
    laptop_key: u64,
    studio_key: u64,
}

fn setup() -> Setup {
    let local = recording(true);
    let router = HostRouter::new(
        Some(InProcessHost::new(
            info(LOCAL, "this-mac"),
            Arc::clone(&local) as Arc<dyn Host>,
        )),
        PathBuf::from("/daemon/nh"),
    );
    let laptop = recording(true);
    let studio = recording(false);
    let laptop_key = router.attach(info(LAPTOP, "laptop"), Arc::clone(&laptop) as Arc<dyn Host>);
    let studio_key = router.attach(info(STUDIO, "studio"), Arc::clone(&studio) as Arc<dyn Host>);
    Setup {
        router,
        local,
        laptop,
        studio,
        laptop_key,
        studio_key,
    }
}

fn approval(interaction: Option<InteractionId>) -> HostNotification {
    HostNotification {
        title: "Claude Code asks for permission".to_owned(),
        body: "Run `pnpm test`".to_owned(),
        urgency: Urgency::High,
        interaction,
        pane: None,
        actions: vec![NotificationAction::Allow, NotificationAction::Deny],
    }
}

fn interaction(n: u128) -> InteractionId {
    InteractionId::new(Ulid(n))
}

fn notified(host: &RecordingHost) -> Vec<(NotificationId, HostNotification)> {
    host.calls()
        .into_iter()
        .filter_map(|call| match call {
            HostCall::Notify(id, n) => Some((id, n)),
            _ => None,
        })
        .collect()
}

fn window(n: u128, client: Option<Ulid>) -> Window {
    Window {
        id: WindowId::new(Ulid(n)),
        bounds: None,
        tab: None,
        by_workspace: Default::default(),
        explicit: None,
        popout: None,
        focused_pane: None,
        facts: WindowFacts::default(),
        reveal: None,
        client,
        single: false,
    }
}

#[test]
fn every_client_gets_a_notification_and_the_core_keeps_one_id() {
    let s = setup();
    let id = s.router.notify(approval(Some(interaction(7))));
    let local = notified(&s.local);
    let laptop = notified(&s.laptop);
    let studio = notified(&s.studio);
    assert_eq!((local.len(), laptop.len(), studio.len()), (1, 1, 1));
    // Each host gives its own id. The core sees none of them.
    assert_ne!(local[0].0, id);
    assert_ne!(laptop[0].0, local[0].0);
    // A client whose notifications have no buttons gets none.
    assert_eq!(local[0].1.actions.len(), 2);
    assert_eq!(laptop[0].1.actions.len(), 2);
    assert!(studio[0].1.actions.is_empty());

    // The update and the withdraw follow the map to the id of each host.
    let mut changed = approval(Some(interaction(7)));
    changed.body = "Run `pnpm test` (and 1 more)".to_owned();
    s.router.update_notification(id, changed.clone());
    s.router.withdraw_notification(id);
    for (host, (host_id, _)) in [(&s.local, &local[0]), (&s.laptop, &laptop[0])] {
        let calls = host.calls();
        assert!(calls.contains(&HostCall::UpdateNotification(*host_id, changed.clone())));
        assert!(calls.contains(&HostCall::WithdrawNotification(*host_id)));
    }
    let mut plain = changed;
    plain.actions.clear();
    assert!(
        s.studio
            .calls()
            .contains(&HostCall::UpdateNotification(studio[0].0, plain))
    );
    // A withdrawn notification is not sent again to a new client.
    let late = recording(true);
    s.router
        .attach(info(Ulid(9), "late"), Arc::clone(&late) as Arc<dyn Host>);
    assert!(notified(&late).is_empty());
}

#[test]
fn the_core_writes_the_actions_and_the_router_fits_them_to_each_client() {
    let s = setup();
    assert!(s.router.notification_capabilities().actions);
    // A daemon with no client at all says the same, so the text that it keeps has the buttons.
    let daemon = HostRouter::new(None, PathBuf::from("/daemon/nh"));
    assert!(daemon.notification_capabilities().actions);
}

#[test]
fn a_new_client_gets_the_badge_the_tray_and_the_open_notifications() {
    let s = setup();
    s.router.set_badge(2);
    let tray = TrayModel {
        entries: vec![TrayEntry {
            label: "Claude Code: Run `pnpm test`".to_owned(),
            interaction: Some(interaction(7)),
            approval: true,
        }],
        badge: 2,
    };
    s.router.set_tray(tray.clone());
    let open = s.router.notify(approval(Some(interaction(7))));
    // A note from a terminal (OSC 9) has no interaction: it is shown once and not kept.
    s.router.notify(HostNotification {
        title: "build".to_owned(),
        body: "done".to_owned(),
        urgency: Urgency::Low,
        interaction: None,
        pane: None,
        actions: Vec::new(),
    });
    for host in [&s.local, &s.laptop, &s.studio] {
        let calls = host.calls();
        assert!(calls.contains(&HostCall::SetBadge(2)));
        assert!(calls.contains(&HostCall::SetTray(tray.clone())));
        assert_eq!(notified(host).len(), 2);
    }

    let late = recording(false);
    s.router
        .attach(info(Ulid(9), "late"), Arc::clone(&late) as Arc<dyn Host>);
    let calls = late.calls();
    assert_eq!(calls[0], HostCall::SetBadge(2));
    assert_eq!(calls[1], HostCall::SetTray(tray));
    let replayed = notified(&late);
    assert_eq!(
        replayed.len(),
        1,
        "only the notification of the interaction"
    );
    assert_eq!(late.open_batches(), [1], "in one request");
    assert_eq!(replayed[0].1.interaction, Some(interaction(7)));
    assert!(
        replayed[0].1.actions.is_empty(),
        "this client has no buttons"
    );
    // The replayed notification is in the map: the withdraw reaches it.
    s.router.withdraw_notification(open);
    assert!(
        late.calls()
            .contains(&HostCall::WithdrawNotification(replayed[0].0))
    );
}

#[test]
fn the_open_notifications_go_to_a_new_client_in_one_request() {
    let s = setup();
    let ids: Vec<NotificationId> = (0..12)
        .map(|n| s.router.notify(approval(Some(interaction(100 + n)))))
        .collect();
    let late = recording(true);
    s.router
        .attach(info(Ulid(9), "late"), Arc::clone(&late) as Arc<dyn Host>);
    // A client counts one request of the attach once in its limit of 10 notifications in 10 s,
    // so it shows all 12.
    assert_eq!(late.open_batches(), [12]);
    let replayed = notified(&late);
    assert_eq!(replayed.len(), 12);
    // Each one is in the map: the withdraw of the last one reaches the new client.
    let last = replayed
        .iter()
        .find(|(_, n)| n.interaction == Some(interaction(111)))
        .expect("the last approval is shown")
        .0;
    s.router.withdraw_notification(ids[11]);
    assert!(late.calls().contains(&HostCall::WithdrawNotification(last)));
    // A client that attaches when nothing is open gets no request.
    for id in ids {
        s.router.withdraw_notification(id);
    }
    let empty = recording(true);
    s.router
        .attach(info(Ulid(10), "empty"), Arc::clone(&empty) as Arc<dyn Host>);
    assert!(empty.open_batches().is_empty());
}

#[test]
fn a_detached_client_gets_nothing_more() {
    let s = setup();
    let id = s.router.notify(approval(Some(interaction(7))));
    let gone = s
        .router
        .detach(s.laptop_key)
        .expect("the laptop was attached");
    assert_eq!(gone.info.device, LAPTOP);
    assert!(!gone.still_connected);
    assert!(
        s.router.detach(s.laptop_key).is_none(),
        "a second detach does nothing"
    );
    let before = s.laptop.calls().len();
    s.router.set_badge(5);
    s.router.withdraw_notification(id);
    s.router.notify(approval(Some(interaction(8))));
    assert!(!s.router.clipboard_write_for(Some(LAPTOP), "x".to_owned()));
    assert_eq!(s.laptop.calls().len(), before);
    assert!(!s.router.is_connected(LAPTOP));
    assert!(s.router.is_connected(STUDIO));
    // The others still get everything.
    assert!(s.studio.calls().contains(&HostCall::SetBadge(5)));
    assert!(s.local.calls().contains(&HostCall::SetBadge(5)));
    let _ = s.studio_key;
}

#[test]
fn a_device_with_two_links_stays_connected_until_both_end() {
    let s = setup();
    let second = recording(true);
    let key = s
        .router
        .attach(info(LAPTOP, "laptop"), Arc::clone(&second) as Arc<dyn Host>);
    let gone = s.router.detach(s.laptop_key).expect("attached");
    assert!(gone.still_connected);
    // The newest link of the device gets the requests for it.
    assert!(
        s.router
            .clipboard_write_for(Some(LAPTOP), "copied".to_owned())
    );
    assert!(
        second
            .calls()
            .contains(&HostCall::ClipboardWrite("copied".to_owned()))
    );
    assert!(!s.router.detach(key).expect("attached").still_connected);
}

#[test]
fn a_window_request_goes_to_the_client_that_owns_the_row() {
    let s = setup();
    let layout = Layout {
        windows: vec![
            window(10, Some(LAPTOP)),
            window(11, Some(LOCAL)),
            window(12, None),
            window(13, Some(Ulid(99))),
        ],
        ..Layout::default()
    };
    s.router.set_owners(&layout);
    let id = |n: u128| WindowId::new(Ulid(n));
    s.router.focus_window(id(10));
    s.router.focus_window(id(11));
    s.router.focus_window(id(12));
    s.router.close_window(id(13));
    assert_eq!(
        s.laptop
            .calls()
            .into_iter()
            .filter(|c| matches!(c, HostCall::FocusWindow(_)))
            .collect::<Vec<_>>(),
        [HostCall::FocusWindow(id(10))]
    );
    // A row without an owner belongs to the host in the process.
    let local: Vec<HostCall> = s
        .local
        .calls()
        .into_iter()
        .filter(|c| matches!(c, HostCall::FocusWindow(_) | HostCall::CloseWindow(_)))
        .collect();
    assert_eq!(
        local,
        [HostCall::FocusWindow(id(11)), HostCall::FocusWindow(id(12))]
    );
    // The owner of the last row is not connected: nobody gets the request.
    for host in [&s.local, &s.laptop, &s.studio] {
        assert!(!host.calls().contains(&HostCall::CloseWindow(id(13))));
    }
}

#[test]
fn the_clipboard_and_the_settings_go_to_the_client_that_the_caller_names() {
    let s = setup();
    assert!(s.router.clipboard_write_for(Some(STUDIO), "a".to_owned()));
    assert!(s.router.clipboard_write_for(None, "b".to_owned()));
    assert!(!s.router.clipboard_write_for(Some(Ulid(99)), "c".to_owned()));
    s.router.open_settings_for(Some(LAPTOP));
    s.router.open_settings_for(None);
    assert_eq!(s.studio.calls(), [HostCall::ClipboardWrite("a".to_owned())]);
    assert!(s.laptop.calls().contains(&HostCall::OpenSettings));
    assert!(
        !s.laptop
            .calls()
            .iter()
            .any(|c| matches!(c, HostCall::ClipboardWrite(_)))
    );
    let local = s.local.calls();
    assert!(local.contains(&HostCall::ClipboardWrite("b".to_owned())));
    assert!(local.contains(&HostCall::OpenSettings));
    assert!(!local.contains(&HostCall::ClipboardWrite("c".to_owned())));
}

#[test]
fn no_host_writes_the_clipboard_of_a_phone() {
    let s = setup();
    let phone = recording(false);
    let key = s.router.attach_as(
        info(Ulid(7), "iphone"),
        Arc::clone(&phone) as Arc<dyn Host>,
        true,
    );
    // The page of the phone writes its clipboard in a tap: the text is dropped.
    assert!(s.router.is_phone(Some(Ulid(7))));
    assert!(!s.router.is_phone(Some(LAPTOP)));
    assert!(!s.router.is_phone(None));
    assert!(!s.router.clipboard_write_for(Some(Ulid(7)), "x".to_owned()));
    assert!(phone.calls().is_empty());
    s.router.detach(key);
    assert!(!s.router.is_phone(Some(Ulid(7))));
}

#[test]
fn the_folder_panel_goes_to_the_host_of_the_window_and_a_link_answers_unsupported() {
    let s = setup();
    s.local.answer_folder(Some(PathBuf::from("/work")));
    let layout = Layout {
        windows: vec![window(10, Some(LOCAL)), window(11, Some(STUDIO))],
        ..Layout::default()
    };
    s.router.set_owners(&layout);
    let pick = |n: u128| FolderPick {
        window: WindowId::new(Ulid(n)),
        start: None,
        title: None,
    };
    let (tx, rx) = std::sync::mpsc::channel();
    s.router.pick_folder(
        pick(10),
        Box::new(move |chosen| {
            let _ = tx.send(chosen);
        }),
    );
    assert_eq!(rx.recv().ok(), Some(Some(PathBuf::from("/work"))));
    // The studio has no panel: it drops `done`, so the channel closes with no answer.
    let (tx, rx) = std::sync::mpsc::channel::<Option<PathBuf>>();
    s.router.pick_folder(
        pick(11),
        Box::new(move |chosen| {
            let _ = tx.send(chosen);
        }),
    );
    assert!(rx.recv().is_err());
    assert!(s.studio.calls().contains(&HostCall::PickFolder(pick(11))));
    assert!(!s.local.calls().contains(&HostCall::PickFolder(pick(11))));
}

#[test]
fn only_the_host_in_the_process_opens_a_path_and_only_for_its_windows() {
    let s = setup();
    let layout = Layout {
        windows: vec![
            window(10, Some(LOCAL)),
            window(11, Some(STUDIO)),
            window(12, None),
        ],
        ..Layout::default()
    };
    s.router.set_owners(&layout);
    let open = |n: u128| PathOpen {
        window: WindowId::new(Ulid(n)),
        path: PathBuf::from("/work/repo/src/lib.rs"),
        reveal: false,
        with: Some("Zed".to_owned()),
    };
    assert_eq!(s.router.open_path(open(10)), Ok(()));
    // A row with no owner belongs to the host in the process.
    assert_eq!(s.router.open_path(open(12)), Ok(()));
    // The file is not on the studio, so the studio is never asked.
    assert_eq!(s.router.open_path(open(11)), Err(OpenRefused::Unsupported));
    assert!(s.local.calls().contains(&HostCall::OpenPath(open(10))));
    assert!(!s.local.calls().contains(&HostCall::OpenPath(open(11))));
    assert!(
        !s.studio
            .calls()
            .iter()
            .any(|call| matches!(call, HostCall::OpenPath(_)))
    );
    // A core with no host in its process (`nh daemon`) opens nothing.
    let daemon = HostRouter::new(None, PathBuf::from("/daemon/nh"));
    assert_eq!(daemon.open_path(open(10)), Err(OpenRefused::Unsupported));
}

#[test]
fn the_sidecar_and_urls_never_leave_the_machine_of_the_core() {
    let s = setup();
    assert_eq!(s.router.sidecar_path(), PathBuf::from("/app/nh"));
    s.router.open_url("https://example.com".to_owned());
    assert!(
        s.local
            .calls()
            .contains(&HostCall::OpenUrl("https://example.com".to_owned()))
    );
    for host in [&s.laptop, &s.studio] {
        assert!(
            !host
                .calls()
                .iter()
                .any(|c| matches!(c, HostCall::OpenUrl(_)))
        );
    }
    let daemon = HostRouter::new(None, PathBuf::from("/daemon/nh"));
    assert_eq!(daemon.sidecar_path(), PathBuf::from("/daemon/nh"));
}

#[test]
fn a_daemon_without_a_host_drops_what_has_no_client() {
    let daemon = HostRouter::new(None, PathBuf::from("/daemon/nh"));
    assert_eq!(daemon.default_device(), None);
    assert!(!daemon.clipboard_write_for(None, "x".to_owned()));
    daemon.set_badge(1);
    let laptop = recording(true);
    daemon.attach(info(LAPTOP, "laptop"), Arc::clone(&laptop) as Arc<dyn Host>);
    assert_eq!(laptop.calls(), [HostCall::SetBadge(1)]);
    assert_eq!(daemon.default_device(), Some(LAPTOP));
    assert_eq!(daemon.connected(), [info(LAPTOP, "laptop")]);
}

#[test]
fn a_note_for_one_client_goes_to_that_client_only() {
    let s = setup();
    s.router.notify_for(Some(STUDIO), approval(None));
    s.router.notify_for(None, approval(None));
    assert_eq!(notified(&s.studio).len(), 1);
    assert_eq!(notified(&s.local).len(), 1);
    assert!(notified(&s.laptop).is_empty());
    assert_eq!(s.router.default_device(), Some(LOCAL));
    assert_eq!(
        s.router.connected(),
        [
            info(LOCAL, "this-mac"),
            info(LAPTOP, "laptop"),
            info(STUDIO, "studio")
        ]
    );
}
