use std::sync::atomic::{AtomicU64, Ordering};

use nh_proto::api::{
    calls::workspace::{TabCreate, WorkspaceCreate},
    common::SplitDirection,
    ids::{PaneId, Ref},
};
use nh_store::Store;

use super::*;
use crate::layout::{
    model::{Alloc, Layout, LayoutError, PaneEntry, Runtime},
    testing::r,
    tree,
};

/// Ids from one counter for the whole test run, so that two commands never make the same id.
static NEXT_ID: AtomicU64 = AtomicU64::new(10_000);

/// Handle numbers from the counters of a real transaction, as the handlers get them.
struct TxAlloc<'a, 'b>(&'a Tx<'b>);

impl Alloc for TxAlloc<'_, '_> {
    fn ulid(&mut self) -> Result<Ulid, LayoutError> {
        Ok(Ulid::from(u128::from(
            NEXT_ID.fetch_add(1, Ordering::Relaxed),
        )))
    }

    fn number(&mut self, scope: &str) -> Result<u32, LayoutError> {
        self.0
            .next_handle(scope)
            .map_err(|e| LayoutError::Internal(e.to_string()))
    }
}

fn ws(name: &str) -> WorkspaceCreate {
    WorkspaceCreate {
        name: Some(name.to_owned()),
        root: Some("/work".into()),
        group: None,
        machine: None,
        command: None,
    }
}

fn tab(workspace: &str) -> TabCreate {
    TabCreate {
        workspace: r(workspace),
        name: None,
        command: None,
    }
}

/// Runs a command on a copy of `layout`, writes the difference, commits, and returns the new
/// layout. This is what a handler does.
fn apply(
    store: &mut Store,
    layout: &Layout,
    command: impl FnOnce(&mut Layout, &mut dyn Alloc),
) -> Layout {
    let mut next = layout.clone();
    store
        .tx(1_000, |tx| {
            command(&mut next, &mut TxAlloc(tx));
            persist(tx, layout, &next)
        })
        .expect("a transaction");
    next.validate().expect("valid");
    next
}

fn reload(store: &Store) -> Layout {
    Layout::from_rows(store.load_layout().expect("rows")).expect("a layout")
}

#[test]
fn what_a_command_changed_is_what_the_next_start_reads() {
    let mut store = Store::open_in_memory().expect("a store");
    let mut layout = apply(&mut store, &Layout::default(), |l, a| {
        let g = l.create_group("Work", a).expect("a group").value;
        let mut p = ws("Main");
        p.group = Some(Ref::id(g));
        l.create_workspace(p, a).expect("a workspace");
    });
    assert_eq!(reload(&store), layout);
    layout = apply(&mut store, &layout, |l, a| {
        let mut p = tab("w1");
        p.name = Some("Logs".into());
        l.create_tab(p, a).expect("a tab");
    });
    assert_eq!(reload(&store), layout);
    assert_eq!(layout.workspaces[0].tabs.len(), 2);
    assert_eq!(layout.workspaces[0].tabs[1].name, "Logs");
}

#[test]
fn nothing_changed_writes_nothing() {
    let mut store = Store::open_in_memory().expect("a store");
    let layout = apply(&mut store, &Layout::default(), |l, a| {
        l.create_workspace(ws("A"), a).expect("created");
    });
    let before = store.load_layout().expect("rows");
    let same = apply(&mut store, &layout, |_, _| {});
    assert_eq!(same, layout);
    assert_eq!(store.load_layout().expect("rows"), before);
}

#[test]
fn closing_things_deletes_their_rows_and_keeps_the_handles_used() {
    let mut store = Store::open_in_memory().expect("a store");
    let mut layout = apply(&mut store, &Layout::default(), |l, a| {
        l.create_workspace(ws("A"), a).expect("created");
        l.create_workspace(ws("B"), a).expect("created");
    });
    layout = apply(&mut store, &layout, |l, a| {
        l.create_tab(tab("w1"), a).expect("a tab");
    });
    // Close one tab: its pane row goes. Close a workspace: its tabs and panes go.
    layout = apply(&mut store, &layout, |l, _| {
        l.close_tab(&r("w1:t2"), false).expect("closed");
    });
    let rows = store.load_layout().expect("rows");
    assert_eq!((rows.tabs.len(), rows.panes.len()), (2, 2));
    layout = apply(&mut store, &layout, |l, _| {
        l.close_workspace(&r("w1"), false).expect("closed");
    });
    let rows = store.load_layout().expect("rows");
    assert_eq!(rows.workspaces.len(), 1);
    assert_eq!((rows.tabs.len(), rows.panes.len()), (1, 1));
    assert_eq!(reload(&store), layout);
    // The handles of the closed workspace are used for good: the next workspace is w3, and a new
    // tab in w2 is w2:t2.
    layout = apply(&mut store, &layout, |l, a| {
        let created = l.create_workspace(ws("C"), a).expect("created").value;
        assert_eq!(created.handle, "w3");
        let again = l.create_tab(tab("w2"), a).expect("a tab").value;
        assert_eq!(again.handle, "w2:t2");
    });
    assert_eq!(reload(&store), layout);
}

#[test]
fn a_tab_that_moves_to_another_workspace_takes_its_panes_along() {
    let mut store = Store::open_in_memory().expect("a store");
    let layout = apply(&mut store, &Layout::default(), |l, a| {
        l.create_workspace(ws("A"), a).expect("created");
        l.create_workspace(ws("B"), a).expect("created");
    });
    let layout = apply(&mut store, &layout, |l, _| {
        l.move_tab(&r("w1:t1"), Some(&r("w2")), Some(0))
            .expect("moved");
    });
    let rows = store.load_layout().expect("rows");
    let pane = &rows
        .panes
        .iter()
        .find(|p| p.slot.handle == "w1:p1")
        .expect("the pane")
        .slot;
    assert_eq!(
        pane.workspace,
        layout.workspaces[1].id.ulid(),
        "the row follows the tab"
    );
    assert_eq!(pane.handle, "w1:p1", "the handle stays");
    assert_eq!(reload(&store), layout);
}

#[test]
fn ratios_zoom_selection_and_order_survive_a_reload() {
    let mut store = Store::open_in_memory().expect("a store");
    let mut layout = apply(&mut store, &Layout::default(), |l, a| {
        l.create_workspace(ws("A"), a).expect("created");
        l.create_workspace(ws("B"), a).expect("created");
        l.create_workspace(ws("C"), a).expect("created");
        l.create_tab(tab("w1"), a).expect("a tab");
    });
    // A split inside a tab, made with the tree operations, then the commands on top.
    layout = apply(&mut store, &layout, |l, _| {
        let workspace = l.workspaces[0].id;
        let tab_id = l.workspaces[0].tabs[0].id;
        let first = tree::panes(&l.workspaces[0].tabs[0].layout)[0];
        let new = PaneId::new(Ulid::from(70_001u128));
        tree::split(
            &mut l.workspaces[0].tabs[0].layout,
            first,
            new,
            SplitDirection::Right,
            None,
        )
        .expect("split");
        l.panes.insert(
            new,
            PaneEntry {
                id: new,
                handle: "w1:p9".into(),
                render_id: 90,
                workspace,
                tab: tab_id,
                cols: 80,
                rows: 24,
                spec: None,
                cwd: None,
                runtime: Runtime::Created,
                facts: Default::default(),
                run: None,
            },
        );
        l.set_ratios(&Ref::id(tab_id), &[], &[0.3, 0.7])
            .expect("ratios");
        l.zoom(&Ref::id(tab_id), Some(&Ref::id(new))).expect("zoom");
        l.select_tab(&r("w1:t1"), None).expect("selected");
        l.move_workspace(&r("w3"), None, Some(&r("w1")))
            .expect("moved");
    });
    let loaded = reload(&store);
    assert_eq!(loaded, layout);
    let handles: Vec<_> = loaded
        .workspaces
        .iter()
        .map(|w| w.handle.as_str())
        .collect();
    assert_eq!(handles, ["w3", "w1", "w2"]);
    assert_eq!(
        loaded.workspaces[1].selected_tab,
        Some(loaded.workspaces[1].tabs[0].id)
    );
    assert!(loaded.workspaces[1].tabs[0].zoomed.is_some());
}

#[test]
fn a_group_that_closes_leaves_its_workspaces_without_it() {
    let mut store = Store::open_in_memory().expect("a store");
    let layout = apply(&mut store, &Layout::default(), |l, a| {
        let g = l.create_group("G", a).expect("a group").value;
        let mut p = ws("A");
        p.group = Some(Ref::id(g));
        l.create_workspace(p, a).expect("created");
    });
    let layout = apply(&mut store, &layout, |l, _| {
        let g = l.groups[0].id;
        l.close_group(&Ref::id(g)).expect("closed");
    });
    let loaded = reload(&store);
    assert_eq!(loaded, layout);
    assert!(loaded.groups.is_empty() && loaded.workspaces[0].group.is_none());
}

#[test]
fn panes_windows_and_popouts_survive_a_reload() {
    use nh_proto::api::{
        calls::{pane::PaneSplit, workspace::WindowOpen},
        common::Bounds,
    };
    let mut store = Store::open_in_memory().expect("a store");
    let mut layout = apply(&mut store, &Layout::default(), |l, a| {
        l.create_workspace(ws("A"), a).expect("created");
        l.create_workspace(ws("B"), a).expect("created");
    });
    layout = apply(&mut store, &layout, |l, a| {
        for direction in [SplitDirection::Right, SplitDirection::Down] {
            l.split_pane(
                PaneSplit {
                    pane: r("w1:p1"),
                    direction,
                    ratio: Some(0.3),
                    command: None,
                },
                a,
            )
            .expect("a split");
        }
        l.float_pane(&r("w1:p2"), None).expect("floated");
        l.popout_pane(&r("w1:p3"), a).expect("popped out");
        l.open_window(
            WindowOpen {
                panes: vec![],
                tab: Some(r("w2:t1")),
                bounds: Some(Bounds {
                    x: 5,
                    y: 6,
                    width: 700,
                    height: 500,
                }),
            },
            a,
        )
        .expect("a window");
        l.open_window(
            WindowOpen {
                panes: vec![r("w1:p1"), r("w2:p1")],
                tab: None,
                bounds: None,
            },
            a,
        )
        .expect("a window");
    });
    assert_eq!(layout.windows.len(), 3);
    let loaded = reload(&store);
    assert_eq!(loaded, layout);
    // The popped-out pane is in no tab, and its window is the one that shows it.
    let popped = loaded.resolve_pane(&r("w1:p3")).expect("p3");
    assert!(matches!(
        loaded.placement(popped),
        Some(crate::layout::model::Placement::PoppedOut(_))
    ));
    assert_eq!(
        loaded.windows.iter().filter(|w| w.popout.is_some()).count(),
        1
    );
    assert_eq!(loaded.windows[1].bounds.map(|b| b.width), Some(700));
    assert_eq!(loaded.windows[2].explicit.as_ref().map(Vec::len), Some(2));
    loaded.validate().expect("valid");

    // Closing the popout window puts the pane back in its tab. A closed pane takes its window.
    layout = apply(&mut store, &layout, |l, _| {
        let window = l.windows[0].id;
        l.close_window(window).expect("closed");
    });
    assert_eq!(reload(&store), layout);
    layout = apply(&mut store, &layout, |l, _| {
        l.close_pane(&nh_proto::api::calls::pane::PaneClose {
            pane: r("w2:p1"),
            force: true,
        })
        .expect("closed");
    });
    let loaded = reload(&store);
    assert_eq!(loaded, layout);
    assert_eq!(
        loaded.windows[1].explicit.as_ref().map(Vec::len),
        Some(1),
        "the closed pane left the list"
    );
}

#[test]
fn a_pane_that_restarts_gets_a_clean_row_and_the_new_command() {
    let mut store = Store::open_in_memory().expect("a store");
    let layout = apply(&mut store, &Layout::default(), |l, a| {
        l.create_workspace(ws("A"), a).expect("created");
    });
    let pane = layout.resolve_pane(&r("w1:p1")).expect("p1");
    // A holder ran, left a snapshot and a token, and the child ended with code 2.
    let record = nh_store::PaneRecord {
        id: pane.ulid(),
        render_id: layout.panes[&pane].render_id,
        socket: "/tmp/h.sock".into(),
        holder_pid: 4242,
        secret: [7; 32],
        fencing_token: 1,
        argv: vec!["sh".into()],
        cwd: "/work".into(),
        cols: 80,
        rows: 24,
        liveness: nh_store::PaneLiveness::Running,
        exit: None,
    };
    assert!(
        store
            .attach_holder(&record, Some("tok"), 5)
            .expect("attached")
    );
    store
        .save_snapshot(pane.ulid(), 100, &[1], (80, 24), 6)
        .expect("a snapshot");
    store
        .set_liveness(
            pane.ulid(),
            nh_store::PaneLiveness::Exited,
            Some(nh_proto::holder::ExitStatus::Code(2)),
            7,
        )
        .expect("exited");
    let ended = reload(&store);
    assert_eq!(ended.panes[&pane].runtime, Runtime::Exited(Some(2)));

    let command = nh_proto::api::common::ProcessSpec {
        argv: vec!["vim".to_owned()],
        ..Default::default()
    };
    let restarted = apply(&mut store, &ended, |l, _| {
        l.restart_pane(&r("w1:p1"), Some(command)).expect("restart");
    });
    assert_eq!(restarted.panes[&pane].runtime, Runtime::Created);
    let back = reload(&store);
    assert_eq!(back.panes[&pane].runtime, Runtime::Created);
    assert_eq!(
        back.panes[&pane].spec.as_ref().map(|s| s.argv.clone()),
        Some(vec!["vim".to_owned()])
    );
    assert!(store.snapshots(pane.ulid()).expect("a read").is_empty());
    assert!(store.pane_tokens().expect("tokens").is_empty());
    assert!(store.pane(pane.ulid()).expect("a read").is_none());
}
