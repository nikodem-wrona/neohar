use serde_json::json;

use super::*;
use crate::{PaneLiveness, PaneRecord};

fn id(n: u128) -> Ulid {
    Ulid::from(n)
}

fn group(n: u128, name: &str, position: u32) -> GroupRow {
    GroupRow {
        id: id(n),
        name: name.to_owned(),
        parent: None,
        position,
    }
}

fn workspace(n: u128, handle: &str, group: Option<u128>, position: u32) -> WorkspaceRow {
    WorkspaceRow {
        id: id(n),
        handle: handle.to_owned(),
        name: format!("Workspace {n}"),
        root: Some("/work".to_owned()),
        machine: "local".to_owned(),
        group: group.map(id),
        position,
        selected_tab: None,
    }
}

fn tab(n: u128, workspace: u128, handle: &str, position: u32, pane: u128) -> TabRow {
    TabRow {
        id: id(n),
        workspace: id(workspace),
        handle: handle.to_owned(),
        name: format!("Tab {n}"),
        position,
        layout: json!({"type": "pane", "pane": id(pane).to_string()}),
        floating: json!([]),
        zoomed: None,
    }
}

fn slot(n: u128, workspace: u128, tab: u128, handle: &str, render_id: u32) -> PaneSlot {
    PaneSlot {
        id: id(n),
        render_id,
        workspace: id(workspace),
        tab: id(tab),
        handle: handle.to_owned(),
        cols: 80,
        rows: 24,
        spec: Some(json!({"argv": ["htop"], "cwd": "/work"})),
    }
}

fn window(n: u128, position: u32) -> WindowRow {
    WindowRow {
        id: id(n),
        bounds: Some(json!({"x": 10, "y": 20, "width": 800, "height": 600})),
        tab_by_workspace: json!({ id(1).to_string(): id(10).to_string() }),
        visible_panes: vec![id(100)],
        focused_pane: Some(id(100)),
        popout_pane: None,
        position,
        client: None,
    }
}

/// A store with one group, two workspaces, a tab with a pane in the first, and a window.
fn store_with_a_layout() -> Store {
    let mut store = Store::open_in_memory().expect("a store");
    store
        .tx(1_000, |tx| {
            tx.put_group(&group(50, "Work", 1))?;
            tx.put_workspace(&workspace(1, "w1", Some(50), 1))?;
            tx.put_workspace(&workspace(2, "w2", None, 2))?;
            tx.put_tab(&tab(10, 1, "w1:t1", 1, 100))?;
            tx.put_pane_slot(&slot(100, 1, 10, "w1:p1", 1))?;
            tx.put_window(&window(200, 1))?;
            Ok(())
        })
        .expect("a layout");
    store
}

#[test]
fn a_layout_comes_back_as_it_was_written() {
    let store = store_with_a_layout();
    let rows = store.load_layout().expect("a layout");
    assert_eq!(rows.groups, [group(50, "Work", 1)]);
    assert_eq!(
        rows.workspaces,
        [workspace(1, "w1", Some(50), 1), workspace(2, "w2", None, 2)]
    );
    assert_eq!(rows.tabs, [tab(10, 1, "w1:t1", 1, 100)]);
    assert_eq!(rows.panes.len(), 1);
    assert_eq!(rows.panes[0].slot, slot(100, 1, 10, "w1:p1", 1));
    assert_eq!(rows.panes[0].liveness, "created");
    assert_eq!(rows.panes[0].exit_code, None);
    assert_eq!(rows.windows, [window(200, 1)]);
}

#[test]
fn the_selected_tab_and_the_zoomed_pane_are_kept() {
    let mut store = store_with_a_layout();
    store
        .tx(2, |tx| {
            let mut zoomed = tab(10, 1, "w1:t1", 1, 100);
            zoomed.zoomed = Some(id(100));
            tx.put_tab(&zoomed)?;
            tx.select_tab(id(1), Some(id(10)))
        })
        .expect("a selection");
    let rows = store.load_layout().expect("a layout");
    assert_eq!(rows.workspaces[0].selected_tab, Some(id(10)));
    assert_eq!(rows.workspaces[1].selected_tab, None);
    assert_eq!(rows.tabs[0].zoomed, Some(id(100)));
    // A tab that closes leaves no selection behind.
    store.tx(3, |tx| tx.delete_tab(id(10))).expect("a close");
    assert_eq!(
        store.load_layout().expect("a layout").workspaces[0].selected_tab,
        None
    );
}

#[test]
fn a_list_comes_back_in_the_order_of_its_positions() {
    let mut store = Store::open_in_memory().expect("a store");
    store
        .tx(1, |tx| {
            for (n, position) in [(1, 3), (2, 1), (3, 12), (4, 2)] {
                tx.put_workspace(&workspace(n, &format!("w{n}"), None, position))?;
            }
            Ok(())
        })
        .expect("workspaces");
    let order: Vec<u128> = store
        .load_layout()
        .expect("a layout")
        .workspaces
        .iter()
        .map(|w| w.id.0)
        .collect();
    // 1, 2, 3 and 12: the text of the key sorts like the number.
    assert_eq!(order, [2, 4, 1, 3]);
}

#[test]
fn putting_a_row_again_changes_it() {
    let mut store = store_with_a_layout();
    store
        .tx(2_000, |tx| {
            let mut renamed = workspace(1, "w1", None, 3);
            renamed.name = "Renamed".to_owned();
            tx.put_workspace(&renamed)?;
            let mut changed = tab(10, 1, "w1:t1", 1, 100);
            changed.layout = json!({"type": "split", "axis": "row", "ratios": [0.5, 0.5],
                "children": [{"type": "pane", "pane": id(100).to_string()},
                             {"type": "pane", "pane": id(101).to_string()}]});
            tx.put_tab(&changed)?;
            tx.put_group(&group(50, "Renamed group", 1))?;
            Ok(())
        })
        .expect("changes");
    let rows = store.load_layout().expect("a layout");
    assert_eq!(rows.workspaces[1].name, "Renamed");
    assert_eq!(
        rows.workspaces[1].group, None,
        "the group is gone from the row"
    );
    assert_eq!(rows.tabs[0].layout["type"], "split");
    assert_eq!(rows.groups[0].name, "Renamed group");
}

#[test]
fn a_pane_that_no_holder_runs_is_not_a_pane_record_and_counts_for_the_render_ids() {
    let store = store_with_a_layout();
    assert!(store.pane(id(100)).expect("a read").is_none());
    assert!(
        store
            .panes(PaneLiveness::Running)
            .expect("a read")
            .is_empty()
    );
    assert_eq!(store.max_render_id().expect("a max"), 1);
}

#[test]
fn the_placement_of_a_pane_changes_and_the_runtime_stays() {
    let mut store = store_with_a_layout();
    // The pane runtime starts a holder for pane 100: the full record comes in as an upsert of the
    // runtime part. `insert_pane` writes a whole row, so use a second pane for that.
    store
        .tx(3_000, |tx| {
            tx.put_pane_slot(&slot(101, 1, 10, "w1:p2", 2))?;
            tx.sql().execute(
                "UPDATE panes SET socket = '/tmp/s', holder_pid = 42, secret = x'00', fencing_token = 9,
                                  liveness = 'running' WHERE id = ?1",
                [id(101).to_string()],
            )?;
            Ok(())
        })
        .expect("a runtime");
    // The runtime columns are only part-filled by this test, so it reads them by hand below.
    // A change of the placement does not touch the runtime columns.
    store
        .tx(4_000, |tx| {
            let mut moved = slot(101, 1, 10, "w1:p2", 2);
            moved.cols = 120;
            moved.rows = 40;
            tx.put_pane_slot(&moved)?;
            Ok(())
        })
        .expect("a move");
    let (holder, socket, cols): (i64, String, i64) = store
        .conn
        .query_row(
            "SELECT holder_pid, socket, cols FROM panes WHERE id = ?1",
            [id(101).to_string()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .expect("a pane");
    assert_eq!((holder, socket.as_str(), cols), (42, "/tmp/s", 120));
    let rows = store.load_layout().expect("a layout");
    let running = rows
        .panes
        .iter()
        .find(|p| p.slot.id == id(101))
        .expect("the pane");
    assert_eq!(running.liveness, "running");
}

#[test]
fn a_pane_row_without_a_place_is_not_in_the_layout() {
    let store = Store::open_in_memory().expect("a store");
    store
        .insert_pane(
            &PaneRecord {
                id: id(7),
                render_id: 7,
                socket: "/tmp/s".into(),
                holder_pid: 1,
                secret: [0; 32],
                fencing_token: 1,
                argv: vec!["sh".into()],
                cwd: "/".into(),
                cols: 80,
                rows: 24,
                liveness: PaneLiveness::Running,
                exit: None,
            },
            1,
        )
        .expect("a pane");
    assert!(store.load_layout().expect("a layout").panes.is_empty());
    assert!(store.pane(id(7)).expect("a read").is_some());
}

#[test]
fn closing_a_workspace_takes_its_tabs_and_panes_and_keeps_the_handle_for_good() {
    let mut store = store_with_a_layout();
    store
        .tx(5_000, |tx| {
            tx.sql().execute(
                "INSERT INTO snapshots (pane, offset, created_at, data) VALUES (?1, 1, 1, x'00')",
                [id(100).to_string()],
            )?;
            tx.close_workspace(id(1))
        })
        .expect("a close");
    let rows = store.load_layout().expect("a layout");
    assert_eq!(rows.workspaces.len(), 1);
    assert_eq!(rows.workspaces[0].id, id(2));
    assert!(rows.tabs.is_empty() && rows.panes.is_empty());
    let snapshots: i64 = store
        .conn
        .query_row("SELECT count(*) FROM snapshots", [], |r| r.get(0))
        .expect("a count");
    assert_eq!(snapshots, 0, "the snapshots went with the pane");
    assert!(!store.workspace_is_open(id(1)).expect("a read"));
    assert!(store.workspace_is_open(id(2)).expect("a read"));
    // The closed row keeps its handle, so no new workspace can take it.
    let again = store.tx(6_000, |tx| tx.put_workspace(&workspace(3, "w1", None, 3)));
    assert!(again.is_err(), "handle w1 is used for good");
    // The window that showed the pane lost its focus, and stays.
    let rows = store.load_layout().expect("a layout");
    assert_eq!(rows.windows[0].focused_pane, None);
}

#[test]
fn closing_a_tab_or_a_pane_removes_the_rows_below() {
    let mut store = store_with_a_layout();
    store
        .tx(2, |tx| {
            tx.put_tab(&tab(11, 1, "w1:t2", 2, 101))?;
            tx.put_pane_slot(&slot(101, 1, 11, "w1:p2", 2))?;
            tx.put_pane_slot(&slot(102, 1, 11, "w1:p3", 3))?;
            Ok(())
        })
        .expect("a second tab");
    store
        .tx(3, |tx| tx.delete_pane(id(102)))
        .expect("a pane closes");
    assert_eq!(store.load_layout().expect("a layout").panes.len(), 2);
    store
        .tx(4, |tx| tx.delete_tab(id(11)))
        .expect("a tab closes");
    let rows = store.load_layout().expect("a layout");
    assert_eq!(rows.tabs.len(), 1);
    assert_eq!(rows.panes.len(), 1);
    assert_eq!(rows.panes[0].slot.id, id(100));
}

#[test]
fn a_group_that_goes_leaves_its_workspaces_without_a_group() {
    let mut store = store_with_a_layout();
    store.tx(2, |tx| tx.delete_group(id(50))).expect("a delete");
    let rows = store.load_layout().expect("a layout");
    assert!(rows.groups.is_empty());
    assert_eq!(rows.workspaces[0].group, None);
}

#[test]
fn a_popout_window_goes_with_its_pane() {
    let mut store = store_with_a_layout();
    store
        .tx(2, |tx| {
            let mut popout = window(201, 2);
            popout.popout_pane = Some(id(100));
            popout.focused_pane = Some(id(100));
            tx.put_window(&popout)
        })
        .expect("a popout window");
    assert_eq!(store.load_layout().expect("a layout").windows.len(), 2);
    store
        .tx(3, |tx| tx.delete_pane(id(100)))
        .expect("a pane closes");
    let rows = store.load_layout().expect("a layout");
    assert_eq!(
        rows.windows.len(),
        1,
        "the popout window went with its pane"
    );
    assert_eq!(rows.windows[0].id, id(200));
}

#[test]
fn rows_need_the_rows_they_point_to() {
    let mut store = Store::open_in_memory().expect("a store");
    // A tab of a workspace that does not exist, and a pane of a tab that does not exist.
    assert!(
        store
            .tx(1, |tx| tx.put_tab(&tab(10, 1, "w1:t1", 1, 100)))
            .is_err()
    );
    assert!(
        store
            .tx(1, |tx| tx.put_pane_slot(&slot(100, 1, 10, "w1:p1", 1)))
            .is_err()
    );
    // The failed transactions left nothing.
    assert_eq!(
        store.load_layout().expect("a layout"),
        LayoutRows::default()
    );
}

#[test]
fn handles_and_render_ids_never_repeat() {
    let mut store = Store::open_in_memory().expect("a store");
    let numbers = |scope: &str, store: &mut Store| -> Vec<u32> {
        let (numbers, _) = store
            .tx(1, |tx| {
                (0..3)
                    .map(|_| tx.next_handle(scope))
                    .collect::<Result<Vec<_>, _>>()
            })
            .expect("numbers");
        numbers
    };
    assert_eq!(numbers("render", &mut store), [1, 2, 3]);
    assert_eq!(numbers("render", &mut store), [4, 5, 6]);
    assert_eq!(
        numbers("pane:w1", &mut store),
        [1, 2, 3],
        "a scope of its own"
    );
}

#[test]
fn a_read_transaction_cannot_write_the_layout() {
    let mut store = store_with_a_layout();
    let result = store.tx_read(1, |tx| tx.put_group(&group(51, "x", 1)));
    assert!(result.is_err());
}

fn record(n: u128) -> PaneRecord {
    PaneRecord {
        id: id(n),
        render_id: 1,
        socket: "/tmp/h.sock".into(),
        holder_pid: 4242,
        secret: [7; 32],
        fencing_token: 99,
        argv: vec!["sh".into(), "-l".into()],
        cwd: "/work".into(),
        cols: 100,
        rows: 30,
        liveness: PaneLiveness::Running,
        exit: None,
    }
}

#[test]
fn a_holder_is_attached_to_a_pane_of_the_layout_and_the_pane_becomes_a_record() {
    let store = store_with_a_layout();
    assert!(store.pane(id(100)).expect("a read").is_none());
    assert!(
        store
            .attach_holder(&record(100), Some("tok-100"), 10)
            .expect("attached")
    );
    let pane = store.pane(id(100)).expect("a read").expect("a record");
    assert_eq!((pane.holder_pid, pane.cols, pane.rows), (4242, 100, 30));
    assert_eq!(pane.argv, ["sh", "-l"]);
    assert_eq!(pane.liveness, PaneLiveness::Running);
    // The placement is intact, and the layout reads the pane as running.
    let rows = store.load_layout().expect("a layout");
    assert_eq!(rows.panes[0].slot.handle, "w1:p1");
    assert_eq!(rows.panes[0].liveness, "running");
    assert_eq!(rows.panes[0].cwd.as_deref(), Some("/work"));
    assert_eq!(store.panes(PaneLiveness::Running).expect("a read").len(), 1);
    // The token comes back with the others.
    assert_eq!(
        store.pane_tokens().expect("tokens"),
        [(id(100), "tok-100".to_owned())]
    );
}

#[test]
fn attaching_clears_how_the_old_child_ended_and_a_pane_with_no_row_is_not_written() {
    let store = store_with_a_layout();
    assert!(
        store
            .attach_holder(&record(100), None, 10)
            .expect("attached")
    );
    store
        .set_liveness(
            id(100),
            PaneLiveness::Exited,
            Some(nh_proto::holder::ExitStatus::Code(3)),
            20,
        )
        .expect("exited");
    let rows = store.load_layout().expect("a layout");
    assert_eq!(
        (rows.panes[0].liveness.as_str(), rows.panes[0].exit_code),
        ("exited", Some(3))
    );
    // A restart attaches a new holder.
    let mut again = record(100);
    again.holder_pid = 5000;
    assert!(
        store
            .attach_holder(&again, Some("tok-2"), 30)
            .expect("attached")
    );
    let rows = store.load_layout().expect("a layout");
    assert_eq!(
        (rows.panes[0].liveness.as_str(), rows.panes[0].exit_code),
        ("running", None)
    );
    assert_eq!(
        store.pane_tokens().expect("tokens"),
        [(id(100), "tok-2".to_owned())]
    );
    // A pane that has no row.
    assert!(!store.attach_holder(&record(777), None, 40).expect("no row"));
    assert!(store.pane(id(777)).expect("a read").is_none());
}

#[test]
fn a_pane_that_starts_again_loses_its_holder_its_token_and_its_snapshots() {
    let mut store = store_with_a_layout();
    assert!(
        store
            .attach_holder(&record(100), Some("tok-100"), 10)
            .expect("attached")
    );
    store
        .save_snapshot(id(100), 500, &[1, 2, 3], (100, 30), 11)
        .expect("a snapshot");
    assert_eq!(store.snapshots(id(100)).expect("a read").len(), 1);

    store.tx(20, |tx| tx.reset_pane(id(100))).expect("reset");
    // The placement stays. The runtime is empty again, as for a pane that was just made.
    let rows = store.load_layout().expect("a layout");
    assert_eq!(rows.panes[0].slot.handle, "w1:p1");
    assert_eq!(rows.panes[0].liveness, "created");
    assert_eq!(rows.panes[0].cwd, None);
    assert!(store.pane(id(100)).expect("a read").is_none());
    assert!(store.snapshots(id(100)).expect("a read").is_empty());
    assert!(store.pane_tokens().expect("tokens").is_empty());
    assert!(
        store
            .panes(PaneLiveness::Running)
            .expect("a read")
            .is_empty()
    );
}

#[test]
fn a_counter_can_be_raised_above_numbers_that_exist_and_never_goes_back() {
    let mut store = Store::open_in_memory().expect("a store");
    let (first, after_raise, after_lower, last) = store
        .tx(1, |tx| {
            let first = tx.next_handle("render")?;
            tx.raise_handle_counter("render", 7)?;
            let after_raise = tx.next_handle("render")?;
            // A lower number changes nothing.
            tx.raise_handle_counter("render", 3)?;
            let after_lower = tx.next_handle("render")?;
            // A counter that does not exist yet starts above the number.
            tx.raise_handle_counter("other", 4)?;
            Ok((first, after_raise, after_lower, tx.next_handle("other")?))
        })
        .expect("a transaction")
        .0;
    assert_eq!((first, after_raise, after_lower, last), (1, 8, 9, 5));
}

#[test]
fn the_answer_for_the_clipboard_writes_of_a_pane_is_stored() {
    let store = store_with_a_layout();
    assert_eq!(store.clipboard_policy(id(100)).expect("a read"), None);
    store
        .set_clipboard_policy(id(100), Some(true), 5)
        .expect("stored");
    assert_eq!(store.clipboard_policy(id(100)).expect("a read"), Some(true));
    store
        .set_clipboard_policy(id(100), Some(false), 6)
        .expect("stored");
    assert_eq!(
        store.clipboard_policy(id(100)).expect("a read"),
        Some(false)
    );
    store
        .set_clipboard_policy(id(100), None, 7)
        .expect("stored");
    assert_eq!(store.clipboard_policy(id(100)).expect("a read"), None);
    // A pane that does not exist asks.
    assert_eq!(store.clipboard_policy(id(999)).expect("a read"), None);
}
