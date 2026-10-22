use nh_proto::api::calls::{pane::PaneSplit, workspace::WorkspaceCreate};
use nh_proto::api::common::SplitDirection;

use super::*;
use crate::layout::testing::{Counters, r};

fn ws(name: &str, root: Option<&str>) -> WorkspaceCreate {
    WorkspaceCreate {
        name: Some(name.to_owned()),
        root: root.map(str::to_owned),
        group: None,
        machine: None,
        command: None,
    }
}

fn spawned(commands: &[RuntimeCmd]) -> Vec<String> {
    commands
        .iter()
        .filter_map(|c| match c {
            RuntimeCmd::Spawn(l) => Some(l.pane.handle.clone()),
            RuntimeCmd::Close(_) => None,
        })
        .collect()
}

#[test]
fn a_new_pane_gets_a_holder_and_a_closed_pane_loses_it() {
    let mut alloc = Counters::new();
    let before = Layout::default();
    let mut layout = before.clone();
    layout
        .create_workspace(ws("A", Some("/work")), &mut alloc)
        .expect("created");
    let started = commands(&before, &layout);
    assert_eq!(spawned(&started), ["w1:p1"]);
    let RuntimeCmd::Spawn(launch) = &started[0] else {
        panic!("a spawn")
    };
    assert_eq!(launch.workspace_handle, "w1");
    assert_eq!(launch.workspace_number, 1);
    assert_eq!(launch.tab_handle, "w1:t1");
    assert_eq!(launch.root.as_deref(), Some("/work"));
    assert_eq!(launch.pane.runtime, Runtime::Created);

    let old = layout.clone();
    layout
        .split_pane(
            PaneSplit {
                pane: r("w1:p1"),
                direction: SplitDirection::Right,
                ratio: None,
                command: None,
            },
            &mut alloc,
        )
        .expect("a split");
    assert_eq!(spawned(&commands(&old, &layout)), ["w1:p2"]);

    let old = layout.clone();
    let pane = layout.resolve_pane(&r("w1:p2")).expect("p2");
    layout
        .close_pane(&nh_proto::api::calls::pane::PaneClose {
            pane: r("w1:p2"),
            force: true,
        })
        .expect("closed");
    assert_eq!(commands(&old, &layout), [RuntimeCmd::Close(pane)]);
}

#[test]
fn a_pane_that_already_runs_or_was_created_before_gets_no_second_holder() {
    let mut alloc = Counters::new();
    let mut layout = Layout::default();
    layout
        .create_workspace(ws("A", None), &mut alloc)
        .expect("created");
    let first = layout.clone();
    // The pane is still `created` in both: the holder is on its way.
    assert!(commands(&first, &layout).is_empty());
    // It runs, and then ends: no command for that.
    let pane = layout.resolve_pane(&r("w1:p1")).expect("p1");
    layout.panes.get_mut(&pane).expect("a pane").runtime = Runtime::Running;
    let running = layout.clone();
    layout.panes.get_mut(&pane).expect("a pane").runtime = Runtime::Exited(Some(0));
    assert!(commands(&running, &layout).is_empty());
    // A restart puts it back to `created`, and the runtime starts a new holder.
    let exited = layout.clone();
    layout.panes.get_mut(&pane).expect("a pane").runtime = Runtime::Created;
    assert_eq!(spawned(&commands(&exited, &layout)), ["w1:p1"]);
}

#[test]
fn a_pane_that_moves_to_another_tab_is_not_started_again() {
    let mut alloc = Counters::new();
    let mut layout = Layout::default();
    layout
        .create_workspace(ws("A", None), &mut alloc)
        .expect("created");
    layout
        .create_workspace(ws("B", None), &mut alloc)
        .expect("created");
    layout
        .split_pane(
            PaneSplit {
                pane: r("w1:p1"),
                direction: SplitDirection::Right,
                ratio: None,
                command: None,
            },
            &mut alloc,
        )
        .expect("a split");
    for pane in layout.panes.values_mut() {
        pane.runtime = Runtime::Running;
    }
    let old = layout.clone();
    layout
        .move_pane(&nh_proto::api::calls::pane::PaneMove {
            pane: r("w1:p2"),
            tab: r("w2:t1"),
            beside: None,
            direction: None,
        })
        .expect("moved");
    assert!(commands(&old, &layout).is_empty());
}
