//! The clients of a core and their windows.
//!
//! A client is the host in the process of the core, or a link client that the server of
//! `link.sock` attaches. The router sends the host requests ([`crate::router`]). This module does
//! what an attach and a detach change in the state:
//!
//! - **Attach.** The core notes when the client was seen (`clients`). The rows of `windows` that
//!   have no owner (rows of a development build from before the windows had a client, or a window
//!   that the CLI opened when no client was there) go to this client. The windows of a client that did not connect for 30 days are
//!   removed.
//! - **Detach.** The windows of the client have no focus and are not visible, so that gate mode
//!   treats their panes as unfocused. The rows stay, and the client gets them back when
//!   it connects again.
//!
//! At its start, the core attaches the host in its process this way. A window whose client is not
//! there at the start is absent too: a machine with no connected client has no focused pane.

use std::sync::Arc;

use nh_proto::api::ids::WindowId;
use nh_store::ClientRow;
use ulid::Ulid;

use crate::{
    actor::{ActorHandle, Parts},
    host::{ClientAttachment, ClientInfo, Host},
    layout::{
        model::{Layout, WindowFacts},
        persist::persist,
        project::{publish, publish_window},
    },
    persist::now_ms,
    render::FocusReport,
    router::HostRouter,
};

/// How long a client may stay away before the core removes its windows.
pub(crate) const FORGET_AFTER_MS: i64 = 30 * 24 * 60 * 60 * 1000;

/// Attaches a link client to the router, and changes the state for it. Dropping the attachment
/// detaches it (`CoreHandle::attach_client`).
pub(crate) fn attach(
    router: &Arc<HostRouter>,
    actor: &ActorHandle,
    info: ClientInfo,
    host: Arc<dyn Host>,
) -> ClientAttachment {
    let key = router.attach(info.clone(), host);
    actor.update_detached(move |mut parts| attached(&mut parts, &info));
    let router = Arc::downgrade(router);
    let actor = actor.clone();
    ClientAttachment {
        detach: Some(Box::new(move || {
            let Some(router) = router.upgrade() else {
                return;
            };
            if let Some(gone) = router.detach(key)
                && !gone.still_connected
            {
                actor.update_detached(move |mut parts| detached(&mut parts, &gone.info));
            }
        })),
    }
}

/// Attaches a paired phone as a client: its window is present while one of its
/// connections is open. A phone has a host that does nothing, and it adopts no window without an
/// owner, because it shows no native window.
pub(crate) fn attach_phone(
    router: &Arc<HostRouter>,
    actor: &ActorHandle,
    info: ClientInfo,
) -> ClientAttachment {
    let key = router.attach_as(
        info.clone(),
        Arc::new(crate::host::NoopHost::default()),
        true,
    );
    actor.update_detached(move |mut parts| touch(&mut parts, &info));
    let router = Arc::downgrade(router);
    let actor = actor.clone();
    ClientAttachment {
        detach: Some(Box::new(move || {
            let Some(router) = router.upgrade() else {
                return;
            };
            if let Some(gone) = router.detach(key)
                && !gone.still_connected
            {
                actor.update_detached(move |mut parts| detached(&mut parts, &gone.info));
            }
        })),
    }
}

/// Removes the windows of a device that is gone: a phone that the person removed.
pub(crate) fn remove_windows_of(parts: &mut Parts<'_>, device: Ulid) {
    let old = parts.state.layout.clone();
    let mut next = old.clone();
    let mut events = Vec::new();
    let gone: Vec<WindowId> = next
        .windows
        .iter()
        .filter(|w| w.client == Some(device))
        .map(|w| w.id)
        .collect();
    if gone.is_empty() {
        return;
    }
    for id in gone {
        match next.close_window(id) {
            Ok(done) => events.extend(done.events),
            Err(error) => tracing::warn!(window = %id, %error, "the window of a phone stays"),
        }
    }
    change_layout(parts, &old, next, events);
}

/// The start of the core: the host in its process attaches, and the windows of the clients that
/// are not there are absent.
pub(crate) fn started(parts: &mut Parts<'_>) {
    let Some(router) = parts.state.host.clone() else {
        return;
    };
    router.set_owners(&parts.state.layout);
    match router.local_info().cloned() {
        Some(local) => attached(parts, &local),
        None => forget_old(parts, &router),
    }
    let absent: Vec<WindowId> = parts
        .state
        .layout
        .windows
        .iter()
        .filter(|w| !w.client.is_some_and(|client| router.is_connected(client)))
        .map(|w| w.id)
        .collect();
    make_absent(parts, &absent);
}

/// A client attached: it was seen now, it adopts the windows without an owner, and the windows of
/// the clients that stayed away too long go.
pub(crate) fn attached(parts: &mut Parts<'_>, info: &ClientInfo) {
    touch(parts, info);
    let old = parts.state.layout.clone();
    let mut next = old.clone();
    let mut adopted = false;
    for window in &mut next.windows {
        if window.client.is_none() {
            window.client = Some(info.device);
            adopted = true;
        }
    }
    if adopted {
        change_layout(parts, &old, next, Vec::new());
    }
    if let Some(router) = parts.state.host.clone() {
        forget_old(parts, &router);
    }
}

/// A client detached: it was seen now, and its windows have no focus and are not visible.
pub(crate) fn detached(parts: &mut Parts<'_>, info: &ClientInfo) {
    touch(parts, info);
    let windows: Vec<WindowId> = parts
        .state
        .layout
        .windows
        .iter()
        .filter(|w| w.client == Some(info.device))
        .map(|w| w.id)
        .collect();
    make_absent(parts, &windows);
}

fn touch(parts: &mut Parts<'_>, info: &ClientInfo) {
    let row = ClientRow {
        device: info.device,
        name: info.name.clone(),
        last_seen: now_ms(),
    };
    if let Err(error) = parts.commit(|tx| tx.touch_client(&row)) {
        tracing::warn!(device = %info.device, %error, "the time of the client is not stored");
    }
}

/// The windows have no focus and are not visible: no pane in them is focused, and the render
/// server sends them no frames. Their place, their tab and their panes stay.
fn make_absent(parts: &mut Parts<'_>, windows: &[WindowId]) {
    let mut changed = false;
    for &id in windows {
        let Some(index) = parts.state.layout.window_index(id) else {
            continue;
        };
        let absent = WindowFacts {
            focused: false,
            visible: false,
            minimized: false,
            refresh_hz: parts.state.layout.windows[index].facts.refresh_hz,
            active_pane: None,
        };
        // Also for a window that was absent already: the report wakes the pacer of the window,
        // which then drops the stream of a link that ended.
        if let Some(render) = &parts.state.render {
            render.focus_report(&FocusReport {
                window: id,
                focused: false,
                visible: false,
                refresh_hz: None,
            });
        }
        if parts.state.layout.windows[index].facts == absent {
            continue;
        }
        parts.state.layout.windows[index].facts = absent;
        publish_window(parts.projections, &parts.state.layout, id);
        changed = true;
    }
    if changed {
        crate::focus::refresh(parts);
    }
}

/// Removes the windows of the clients that did not connect for [`FORGET_AFTER_MS`], and the rows
/// of those clients. A client that is there now is never old.
fn forget_old(parts: &mut Parts<'_>, router: &HostRouter) {
    let before = now_ms() - FORGET_AFTER_MS;
    let old_clients: Vec<Ulid> = match parts.store.clients() {
        Ok(rows) => rows
            .into_iter()
            .filter(|row| row.last_seen < before && !router.is_connected(row.device))
            .map(|row| row.device)
            .collect(),
        Err(error) => {
            tracing::warn!(%error, "the clients are not read");
            return;
        }
    };
    if old_clients.is_empty() {
        return;
    }
    let old = parts.state.layout.clone();
    let mut next = old.clone();
    let mut events = Vec::new();
    let gone: Vec<WindowId> = next
        .windows
        .iter()
        .filter(|w| w.client.is_some_and(|client| old_clients.contains(&client)))
        .map(|w| w.id)
        .collect();
    for id in gone {
        match next.close_window(id) {
            Ok(done) => events.extend(done.events),
            Err(error) => {
                tracing::warn!(window = %id, %error, "the window of an old client stays");
            }
        }
    }
    let stored = parts.commit(|tx| {
        for device in &old_clients {
            tx.delete_client(*device)?;
        }
        Ok(())
    });
    if let Err(error) = stored {
        tracing::warn!(%error, "the old clients are not removed");
        return;
    }
    tracing::info!(
        clients = old_clients.len(),
        "removed the windows of old clients"
    );
    change_layout(parts, &old, next, events);
}

/// Writes a change of the layout that no command made, like a command does: the rows and the
/// events in one transaction, then the projections and the state.
fn change_layout(
    parts: &mut Parts<'_>,
    old: &Layout,
    next: Layout,
    events: Vec<crate::layout::ops::NewEvent>,
) {
    if let Err(error) = next.validate() {
        tracing::error!(%error, "a change of the windows of a client broke a rule");
        return;
    }
    let stored = parts.commit(|tx| {
        persist(tx, old, &next)?;
        for event in &events {
            tx.append(Some(&event.subject), &event.payload)?;
        }
        Ok(())
    });
    if let Err(error) = stored {
        tracing::error!(%error, "a change of the windows of a client is not stored");
        return;
    }
    publish(parts.projections, old, &next);
    parts.state.layout = next;
    parts.state.layout_changed(old);
    crate::agents::runs::sync_windows(
        parts.projections,
        &mut parts.state.runs,
        old,
        &parts.state.layout,
    );
}

#[cfg(test)]
mod tests;
