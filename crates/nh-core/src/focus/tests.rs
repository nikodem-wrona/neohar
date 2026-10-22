//! The focus of the panes: what the facts of the windows mean, and the debounce.

use std::time::Duration;

use nh_proto::api::ids::{PaneId, WindowId};
use serde_json::json;

use crate::{
    agents::tests::{Fixture, Options},
    layout::model::WindowFacts,
};

/// A debounce that a test can wait for.
const SHORT: Duration = Duration::from_millis(150);

fn focused_facts(pane: Option<PaneId>) -> WindowFacts {
    WindowFacts {
        focused: true,
        visible: true,
        active_pane: pane,
        ..WindowFacts::default()
    }
}

async fn report(f: &Fixture, window: WindowId, facts: WindowFacts) {
    f.thread
        .handle()
        .update(move |parts| crate::actor::layout::apply_window_report(parts, window, facts))
        .await
        .expect("the actor answers");
}

async fn focused(f: &Fixture, pane: PaneId) -> bool {
    f.update(move |parts| parts.state.focus.is_focused(pane))
        .await
}

/// Waits until the pane has the wanted focus, or fails after a while.
async fn becomes(f: &Fixture, pane: PaneId, wanted: bool) {
    for _ in 0..200 {
        if focused(f, pane).await == wanted {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the focus of the pane did not become {wanted}");
}

async fn split(f: &Fixture) -> PaneId {
    let made = f
        .call(
            "pane.split",
            json!({"pane": f.pane.to_string(), "direction": "right"}),
        )
        .await;
    made["pane"]
        .as_str()
        .expect("a pane id")
        .parse()
        .expect("a pane id")
}

fn without_debounce() -> Fixture {
    Fixture::with_options(Options::default())
}

fn with_debounce() -> Fixture {
    Fixture::with_options(Options {
        focus_debounce: Some(SHORT),
        ..Options::default()
    })
}

#[tokio::test]
async fn a_pane_is_focused_when_it_is_the_active_element_of_a_focused_window_that_shows_it() {
    let f = without_debounce();
    let window = f.open_window().await;
    assert!(!focused(&f, f.pane).await, "no window reported yet");
    report(&f, window, focused_facts(Some(f.pane))).await;
    assert!(focused(&f, f.pane).await);
}

#[tokio::test]
async fn a_window_that_the_os_did_not_focus_gives_no_focus() {
    let f = without_debounce();
    let window = f.open_window().await;
    let mut facts = focused_facts(Some(f.pane));
    facts.focused = false;
    report(&f, window, facts).await;
    assert!(!focused(&f, f.pane).await, "the app is in the background");
}

#[tokio::test]
async fn a_window_that_cannot_be_seen_gives_no_focus() {
    let f = without_debounce();
    let window = f.open_window().await;
    let mut hidden = focused_facts(Some(f.pane));
    hidden.visible = false;
    report(&f, window, hidden).await;
    assert!(!focused(&f, f.pane).await, "the window is covered");
    let mut minimized = focused_facts(Some(f.pane));
    minimized.minimized = true;
    report(&f, window, minimized).await;
    assert!(!focused(&f, f.pane).await, "the window is minimized");
}

#[tokio::test]
async fn keyboard_focus_in_the_chrome_gives_no_pane_focus() {
    let f = without_debounce();
    let window = f.open_window().await;
    report(&f, window, focused_facts(None)).await;
    assert!(!focused(&f, f.pane).await, "the user types in the palette");
}

#[tokio::test]
async fn typing_in_a_sibling_pane_moves_the_focus_to_the_sibling() {
    let f = without_debounce();
    let sibling = split(&f).await;
    let window = f.open_window().await;
    report(&f, window, focused_facts(Some(f.pane))).await;
    assert!(focused(&f, f.pane).await);
    assert!(!focused(&f, sibling).await);
    report(&f, window, focused_facts(Some(sibling))).await;
    assert!(
        !focused(&f, f.pane).await,
        "the pane is visible, and not focused"
    );
    assert!(focused(&f, sibling).await);
}

#[tokio::test]
async fn a_pane_that_the_window_does_not_show_gives_no_focus() {
    let f = without_debounce();
    let window = f.open_window().await;
    let stranger = PaneId::new(ulid::Ulid::from(77));
    report(&f, window, focused_facts(Some(stranger))).await;
    assert!(!focused(&f, stranger).await);
}

#[tokio::test]
async fn a_change_counts_only_after_the_debounce() {
    let f = with_debounce();
    let window = f.open_window().await;
    report(&f, window, focused_facts(Some(f.pane))).await;
    assert!(!focused(&f, f.pane).await, "the change has not stood yet");
    becomes(&f, f.pane, true).await;
    // The way back is debounced too: a switch to another app and back does not flap.
    let mut away = focused_facts(Some(f.pane));
    away.focused = false;
    report(&f, window, away).await;
    assert!(
        focused(&f, f.pane).await,
        "the pane keeps its focus for a moment"
    );
    becomes(&f, f.pane, false).await;
}

#[tokio::test]
async fn a_change_that_is_undone_within_the_debounce_never_counts() {
    let f = with_debounce();
    let window = f.open_window().await;
    report(&f, window, focused_facts(Some(f.pane))).await;
    report(&f, window, focused_facts(None)).await;
    tokio::time::sleep(SHORT * 3).await;
    assert!(!focused(&f, f.pane).await, "a command-tab through the app");
}

#[tokio::test]
async fn a_closed_window_ends_the_focus_of_its_pane() {
    let f = with_debounce();
    let window = f.open_window().await;
    report(&f, window, focused_facts(Some(f.pane))).await;
    becomes(&f, f.pane, true).await;
    f.call("window.close", json!({"window": window.to_string()}))
        .await;
    becomes(&f, f.pane, false).await;
}

#[tokio::test]
async fn a_phone_window_focuses_its_pane_but_is_not_the_focus_of_a_desktop() {
    let f = without_debounce();
    let window = f.open_window().await;
    let phone = ulid::Ulid::from(77u128);
    f.update(move |parts| {
        parts.state.phones.add_for_test(phone, None);
        parts.state.layout.set_window_client(window, Some(phone));
    })
    .await;
    report(&f, window, focused_facts(Some(f.pane))).await;
    // Gate mode: a pane in focus on a visible phone is focused.
    assert!(focused(&f, f.pane).await);
    // The push: a phone window is not a window of a desktop client.
    assert!(!f.update(|parts| parts.state.focus.desk_focused()).await);

    // The same facts for a window of a desktop client count for both.
    let desktop = ulid::Ulid::from(78u128);
    f.update(move |parts| {
        parts.state.layout.set_window_client(window, Some(desktop));
    })
    .await;
    // A report that changes nothing changes no focus, so the window loses the focus first.
    report(&f, window, WindowFacts::default()).await;
    report(&f, window, focused_facts(Some(f.pane))).await;
    assert!(f.update(|parts| parts.state.focus.desk_focused()).await);
}
