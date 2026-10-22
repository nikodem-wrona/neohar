use nh_proto::api::{
    calls::{
        pane::{PaneClose, PaneMove, PaneSplit},
        workspace::{TabCreate, WindowOpen, WorkspaceCreate},
    },
    common::{Bounds, ProcessSpec, Rect, SplitDirection},
    ids::{PaneId, Ref, TabId, WindowId},
    model::{LayoutNode, PaneLiveness},
};
use proptest::prelude::*;

use super::*;
use crate::layout::{
    model::{DEFAULT_SIZE, Facts, Report, Runtime},
    testing::{Counters, r},
};

fn kinds(events: &[NewEvent]) -> Vec<&'static str> {
    events.iter().map(|e| e.payload.kind().name()).collect()
}

fn ws(name: &str) -> WorkspaceCreate {
    WorkspaceCreate {
        name: Some(name.to_owned()),
        root: None,
        group: None,
        machine: None,
        command: None,
    }
}

fn layout_with(n: usize) -> (Layout, Counters) {
    let mut layout = Layout::default();
    let mut alloc = Counters::new();
    for i in 1..=n {
        layout
            .create_workspace(ws(&format!("W{i}")), &mut alloc)
            .expect("a workspace");
    }
    (layout, alloc)
}

fn split(
    layout: &mut Layout,
    alloc: &mut Counters,
    pane: &str,
    direction: SplitDirection,
) -> PaneId {
    layout
        .split_pane(
            PaneSplit {
                pane: r(pane),
                direction,
                ratio: None,
                command: None,
            },
            alloc,
        )
        .expect("a split")
        .value
        .pane
}

fn close(pane: &str, force: bool) -> PaneClose {
    PaneClose {
        pane: r(pane),
        force,
    }
}

fn tiled(layout: &Layout, tab: &str) -> Vec<String> {
    let (w, t) = layout.resolve_tab(&r(tab)).expect("a tab");
    tree::panes(&layout.workspaces[w].tabs[t].layout)
        .iter()
        .map(|p| layout.panes[p].handle.clone())
        .collect()
}

#[test]
fn a_split_puts_a_new_pane_beside_the_old_one() {
    let (mut layout, mut alloc) = layout_with(1);
    let done = layout
        .split_pane(
            PaneSplit {
                pane: r("w1:p1"),
                direction: SplitDirection::Right,
                ratio: Some(0.4),
                command: Some(ProcessSpec {
                    argv: vec!["htop".into()],
                    cwd: None,
                    env: Default::default(),
                }),
            },
            &mut alloc,
        )
        .expect("a split");
    assert_eq!(done.value.handle, "w1:p2");
    assert_eq!(kinds(&done.events), ["pane.created", "layout.changed"]);
    assert_eq!(tiled(&layout, "w1:t1"), ["w1:p1", "w1:p2"]);
    let LayoutNode::Split { axis, ratios, .. } = &layout.workspaces[0].tabs[0].layout else {
        panic!("a split")
    };
    assert_eq!(*axis, nh_proto::api::common::Axis::Row);
    assert!(
        (ratios[0] - 0.6).abs() < 1e-6 && (ratios[1] - 0.4).abs() < 1e-6,
        "{ratios:?}"
    );
    let new = &layout.panes[&done.value.pane];
    assert_eq!(new.spec.as_ref().expect("a spec").argv, ["htop"]);
    assert_eq!(new.render_id, 2);
    layout.validate().expect("valid");
}

#[test]
fn a_split_ends_a_zoom_and_refuses_a_pane_that_is_not_in_the_tree() {
    let (mut layout, mut alloc) = layout_with(1);
    let first = layout.panes.keys().next().copied().expect("a pane");
    layout
        .zoom(
            &Ref::id(layout.workspaces[0].tabs[0].id),
            Some(&Ref::id(first)),
        )
        .expect("zoomed");
    split(&mut layout, &mut alloc, "w1:p1", SplitDirection::Down);
    assert_eq!(
        layout.workspaces[0].tabs[0].zoomed, None,
        "the new pane must be seen"
    );

    // A floating pane and a popped-out pane cannot be split.
    layout.float_pane(&r("w1:p2"), None).expect("floated");
    let before = layout.clone();
    for target in ["w1:p2", "w1:p9"] {
        let result = layout.split_pane(
            PaneSplit {
                pane: r(target),
                direction: SplitDirection::Right,
                ratio: None,
                command: None,
            },
            &mut alloc,
        );
        assert!(result.is_err(), "{target}");
    }
    // A bad ratio makes no pane.
    let bad = layout.split_pane(
        PaneSplit {
            pane: r("w1:p1"),
            direction: SplitDirection::Right,
            ratio: Some(1.5),
            command: None,
        },
        &mut alloc,
    );
    assert!(matches!(bad, Err(LayoutError::Invalid(_))));
    assert_eq!(layout.panes.len(), before.panes.len());
}

#[test]
fn closing_a_pane_gives_its_room_to_the_others() {
    let (mut layout, mut alloc) = layout_with(1);
    split(&mut layout, &mut alloc, "w1:p1", SplitDirection::Right);
    split(&mut layout, &mut alloc, "w1:p2", SplitDirection::Down);
    assert_eq!(layout.panes.len(), 3);
    let done = layout.close_pane(&close("w1:p2", false)).expect("closed");
    assert_eq!(kinds(&done.events), ["pane.closed", "layout.changed"]);
    assert_eq!(tiled(&layout, "w1:t1"), ["w1:p1", "w1:p3"]);
    assert!(!layout.panes.values().any(|p| p.handle == "w1:p2"));
    layout.validate().expect("valid");
}

#[test]
fn a_live_pane_needs_force_and_a_refused_close_changes_nothing() {
    let (mut layout, mut alloc) = layout_with(1);
    let second = split(&mut layout, &mut alloc, "w1:p1", SplitDirection::Right);
    layout.panes.get_mut(&second).expect("a pane").runtime = Runtime::Running;
    let before = layout.clone();
    let refused = layout.close_pane(&close("w1:p2", false));
    assert!(
        matches!(refused, Err(LayoutError::Confirm { .. })),
        "{refused:?}"
    );
    assert_eq!(layout, before);
    layout.close_pane(&close("w1:p2", true)).expect("forced");
    assert_eq!(layout.panes.len(), 1);
}

#[test]
fn the_last_pane_of_a_tab_closes_the_tab_unless_another_pane_can_take_its_place() {
    // Alone: the tab closes with the pane.
    let (mut layout, _) = layout_with(1);
    let done = layout.close_pane(&close("w1:p1", false)).expect("closed");
    assert_eq!(kinds(&done.events), ["pane.closed", "tab.closed"]);
    assert!(layout.workspaces[0].tabs.is_empty());
    assert_eq!(layout.workspaces[0].selected_tab, None);
    layout.validate().expect("valid");

    // With a floating pane: it takes the place in the tree.
    let (mut layout, mut alloc) = layout_with(1);
    split(&mut layout, &mut alloc, "w1:p1", SplitDirection::Right);
    layout.float_pane(&r("w1:p2"), None).expect("floated");
    layout.close_pane(&close("w1:p1", false)).expect("closed");
    assert_eq!(tiled(&layout, "w1:t1"), ["w1:p2"]);
    assert!(layout.workspaces[0].tabs[0].floating.is_empty());
    layout.validate().expect("valid");

    // With a popped-out pane: it comes back, and its window closes.
    let (mut layout, mut alloc) = layout_with(1);
    split(&mut layout, &mut alloc, "w1:p1", SplitDirection::Right);
    layout
        .popout_pane(&r("w1:p2"), &mut alloc)
        .expect("popped out");
    let done = layout.close_pane(&close("w1:p1", false)).expect("closed");
    assert!(
        kinds(&done.events).contains(&"window.closed"),
        "{:?}",
        kinds(&done.events)
    );
    assert_eq!(tiled(&layout, "w1:t1"), ["w1:p2"]);
    assert!(layout.windows.is_empty());
    layout.validate().expect("valid");
}

#[test]
fn a_pane_moves_to_the_edge_of_another_tab_or_beside_a_pane_of_it() {
    let (mut layout, mut alloc) = layout_with(1);
    split(&mut layout, &mut alloc, "w1:p1", SplitDirection::Right);
    layout
        .create_tab(
            TabCreate {
                workspace: r("w1"),
                name: None,
                command: None,
            },
            &mut alloc,
        )
        .expect("a tab");
    // p2 to the edge of the second tab (p3 is its pane).
    let done = layout
        .move_pane(&PaneMove {
            pane: r("w1:p2"),
            tab: r("w1:t2"),
            beside: None,
            direction: None,
        })
        .expect("moved");
    assert_eq!(
        kinds(&done.events),
        ["pane.moved", "layout.changed", "layout.changed"]
    );
    assert_eq!(tiled(&layout, "w1:t1"), ["w1:p1"]);
    assert_eq!(tiled(&layout, "w1:t2"), ["w1:p3", "w1:p2"]);
    assert_eq!(
        layout.panes[&layout.resolve_pane(&r("w1:p2")).expect("p2")].handle,
        "w1:p2"
    );
    // p1 beside p3, above it. The first tab has no pane left, so it closes.
    let done = layout
        .move_pane(&PaneMove {
            pane: r("w1:p1"),
            tab: r("w1:t2"),
            beside: Some(r("w1:p3")),
            direction: Some(SplitDirection::Up),
        })
        .expect("moved");
    assert!(kinds(&done.events).contains(&"tab.closed"));
    assert_eq!(layout.workspaces[0].tabs.len(), 1);
    assert_eq!(tiled(&layout, "w1:t2").len(), 3);
    layout.validate().expect("valid");
}

#[test]
fn a_pane_moves_to_a_tab_of_another_workspace_and_keeps_its_handle() {
    let (mut layout, mut alloc) = layout_with(2);
    split(&mut layout, &mut alloc, "w1:p1", SplitDirection::Right);
    layout
        .move_pane(&PaneMove {
            pane: r("w1:p2"),
            tab: r("w2:t1"),
            beside: None,
            direction: None,
        })
        .expect("moved");
    let pane = layout
        .resolve_pane(&r("w1:p2"))
        .expect("found by its old handle");
    assert_eq!(layout.panes[&pane].workspace, layout.workspaces[1].id);
    assert_eq!(layout.panes[&pane].tab, layout.workspaces[1].tabs[0].id);
    layout.validate().expect("valid");
}

#[test]
fn a_move_that_makes_no_sense_is_refused() {
    let (mut layout, mut alloc) = layout_with(1);
    split(&mut layout, &mut alloc, "w1:p1", SplitDirection::Right);
    layout
        .create_tab(
            TabCreate {
                workspace: r("w1"),
                name: None,
                command: None,
            },
            &mut alloc,
        )
        .expect("a tab");
    let before = layout.clone();
    let refused = [
        // Beside itself.
        PaneMove {
            pane: r("w1:p2"),
            tab: r("w1:t2"),
            beside: Some(r("w1:p2")),
            direction: None,
        },
        // Beside a pane of another tab.
        PaneMove {
            pane: r("w1:p2"),
            tab: r("w1:t2"),
            beside: Some(r("w1:p1")),
            direction: None,
        },
        // To a tab that does not exist.
        PaneMove {
            pane: r("w1:p2"),
            tab: r("w1:t9"),
            beside: None,
            direction: None,
        },
    ];
    for params in refused {
        assert!(layout.clone().move_pane(&params).is_err());
    }
    // A tab with one pane has nothing to move inside it.
    let alone = layout.move_pane(&PaneMove {
        pane: r("w1:p3"),
        tab: r("w1:t2"),
        beside: None,
        direction: None,
    });
    assert!(matches!(alone, Err(LayoutError::Invalid(_))));
    // Inside a tab, to the edge, works with more than one pane.
    layout
        .move_pane(&PaneMove {
            pane: r("w1:p1"),
            tab: r("w1:t1"),
            beside: None,
            direction: Some(SplitDirection::Right),
        })
        .expect("to the edge");
    assert_eq!(tiled(&layout, "w1:t1"), ["w1:p2", "w1:p1"]);
    let _ = before;
}

#[test]
fn a_pane_floats_with_a_rectangle_and_comes_back_at_the_edge() {
    let (mut layout, mut alloc) = layout_with(1);
    split(&mut layout, &mut alloc, "w1:p1", SplitDirection::Right);
    split(&mut layout, &mut alloc, "w1:p2", SplitDirection::Right);
    let done = layout.float_pane(&r("w1:p2"), None).expect("floated");
    assert_eq!(kinds(&done.events), ["pane.floated", "layout.changed"]);
    let rect = layout.workspaces[0].tabs[0].floating[0].rect;
    assert_eq!((rect.w, rect.h), (0.5, 0.5));
    assert_eq!(tiled(&layout, "w1:t1"), ["w1:p1", "w1:p3"]);
    // The second floating pane is shifted.
    layout
        .float_pane(
            &r("w1:p3"),
            Some(Rect {
                x: 0.1,
                y: 0.1,
                w: 0.3,
                h: 0.3,
            }),
        )
        .expect("floated with a rectangle");
    assert_eq!(layout.workspaces[0].tabs[0].floating[1].rect.x, 0.1);
    layout
        .set_float_rect(
            &r("w1:p2"),
            Rect {
                x: 0.0,
                y: 0.0,
                w: 1.0,
                h: 1.0,
            },
        )
        .expect("a drag");
    assert_eq!(layout.workspaces[0].tabs[0].floating[0].rect.w, 1.0);
    layout.validate().expect("valid");

    // Back in the tree, at the right edge.
    let done = layout.unfloat_pane(&r("w1:p2")).expect("unfloated");
    assert_eq!(kinds(&done.events), ["pane.unfloated", "layout.changed"]);
    assert_eq!(tiled(&layout, "w1:t1"), ["w1:p1", "w1:p2"]);
    layout.validate().expect("valid");
}

#[test]
fn a_drag_causes_no_event_and_only_a_rectangle_inside_the_tab() {
    let (mut layout, mut alloc) = layout_with(1);
    split(&mut layout, &mut alloc, "w1:p1", SplitDirection::Right);
    layout.float_pane(&r("w1:p2"), None).expect("floated");
    let done = layout
        .set_float_rect(
            &r("w1:p2"),
            Rect {
                x: 0.3,
                y: 0.3,
                w: 0.4,
                h: 0.4,
            },
        )
        .expect("a rectangle");
    assert!(done.events.is_empty());
    for bad in [
        Rect {
            x: -0.1,
            y: 0.0,
            w: 0.5,
            h: 0.5,
        },
        Rect {
            x: 0.6,
            y: 0.0,
            w: 0.5,
            h: 0.5,
        },
        Rect {
            x: 0.0,
            y: 0.0,
            w: 0.01,
            h: 0.5,
        },
        Rect {
            x: f32::NAN,
            y: 0.0,
            w: 0.5,
            h: 0.5,
        },
    ] {
        assert!(layout.set_float_rect(&r("w1:p2"), bad).is_err(), "{bad:?}");
    }
    // A pane in the tree has no rectangle to set.
    assert!(
        layout
            .set_float_rect(
                &r("w1:p1"),
                Rect {
                    x: 0.1,
                    y: 0.1,
                    w: 0.3,
                    h: 0.3
                }
            )
            .is_err()
    );
}

#[test]
fn the_only_tiled_pane_cannot_float_or_pop_out_and_the_others_can_only_once() {
    let (mut layout, mut alloc) = layout_with(1);
    assert!(layout.float_pane(&r("w1:p1"), None).is_err());
    assert!(layout.popout_pane(&r("w1:p1"), &mut alloc).is_err());
    split(&mut layout, &mut alloc, "w1:p1", SplitDirection::Right);
    layout.float_pane(&r("w1:p2"), None).expect("floated");
    assert!(
        layout.float_pane(&r("w1:p2"), None).is_err(),
        "it floats already"
    );
    assert!(
        layout.unfloat_pane(&r("w1:p1")).is_err(),
        "it does not float"
    );
}

#[test]
fn a_pane_pops_out_into_a_window_and_comes_back_when_the_window_closes() {
    let (mut layout, mut alloc) = layout_with(1);
    split(&mut layout, &mut alloc, "w1:p1", SplitDirection::Right);
    let done = layout
        .popout_pane(&r("w1:p2"), &mut alloc)
        .expect("popped out");
    assert_eq!(
        kinds(&done.events),
        ["pane.moved", "window.opened", "layout.changed"]
    );
    let window = done.value.window;
    assert_eq!(tiled(&layout, "w1:t1"), ["w1:p1"]);
    let win = &layout.windows[0];
    assert_eq!(
        (win.id, win.popout),
        (window, Some(layout.resolve_pane(&r("w1:p2")).expect("p2")))
    );
    assert_eq!(layout.visible_panes(win), vec![win.popout.expect("a pane")]);
    assert_eq!(
        layout.placement(win.popout.expect("a pane")),
        Some(Placement::PoppedOut(window))
    );
    layout.validate().expect("valid");
    // A popout window shows its pane only.
    assert!(layout.set_visible_panes(window, &[r("w1:p1")]).is_err());
    assert!(
        layout
            .show_tab_in_window(window, layout.workspaces[0].tabs[0].id)
            .is_err()
    );

    let done = layout.close_window(window).expect("closed");
    assert_eq!(
        kinds(&done.events),
        ["window.closed", "pane.moved", "layout.changed"]
    );
    assert_eq!(tiled(&layout, "w1:t1"), ["w1:p1", "w1:p2"]);
    assert!(layout.windows.is_empty());
    layout.validate().expect("valid");
}

#[test]
fn closing_a_popped_out_pane_closes_its_window_and_so_does_closing_its_tab() {
    let (mut layout, mut alloc) = layout_with(1);
    split(&mut layout, &mut alloc, "w1:p1", SplitDirection::Right);
    layout
        .popout_pane(&r("w1:p2"), &mut alloc)
        .expect("popped out");
    let done = layout.close_pane(&close("w1:p2", false)).expect("closed");
    assert!(kinds(&done.events).contains(&"window.closed"));
    assert!(layout.windows.is_empty());
    layout.validate().expect("valid");

    split(&mut layout, &mut alloc, "w1:p1", SplitDirection::Right);
    layout
        .popout_pane(&r("w1:p3"), &mut alloc)
        .expect("popped out");
    let done = layout.close_tab(&r("w1:t1"), false).expect("closed");
    assert!(
        kinds(&done.events).contains(&"window.closed"),
        "{:?}",
        kinds(&done.events)
    );
    assert!(layout.windows.is_empty() && layout.panes.is_empty());
    layout.validate().expect("valid");
}

#[test]
fn a_window_shows_a_tab_or_some_panes() {
    let (mut layout, mut alloc) = layout_with(2);
    split(&mut layout, &mut alloc, "w1:p1", SplitDirection::Right);
    layout.float_pane(&r("w1:p2"), None).expect("floated");
    // No tab and no panes: the selected tab of the first workspace.
    let default = layout
        .open_window(
            WindowOpen {
                panes: vec![],
                tab: None,
                bounds: Some(Bounds {
                    x: 1,
                    y: 2,
                    width: 300,
                    height: 200,
                }),
            },
            &mut alloc,
        )
        .expect("opened");
    assert_eq!(kinds(&default.events), ["window.opened"]);
    let window = &layout.windows[0];
    assert_eq!(window.tab, Some(layout.workspaces[0].tabs[0].id));
    assert_eq!(window.bounds.map(|b| b.width), Some(300));
    // The tab: its tiled and its floating panes.
    assert_eq!(layout.visible_panes(window).len(), 2);
    // A zoom shows one pane.
    layout.zoom(&r("w1:t1"), Some(&r("w1:p2"))).expect("zoomed");
    let window = layout.windows[0].clone();
    assert_eq!(layout.visible_panes(&window).len(), 1);
    layout.zoom(&r("w1:t1"), None).expect("zoom ended");

    // Explicit panes, from two workspaces.
    let explicit = layout
        .open_window(
            WindowOpen {
                panes: vec![r("w1:p1"), r("w2:p1")],
                tab: None,
                bounds: None,
            },
            &mut alloc,
        )
        .expect("opened")
        .value
        .window;
    let win = layout
        .windows
        .iter()
        .find(|w| w.id == explicit)
        .expect("the window")
        .clone();
    assert!(win.tab.is_none());
    assert_eq!(layout.visible_panes(&win).len(), 2);
    // Both a tab and panes is an error, and so is an unknown pane.
    assert!(
        layout
            .open_window(
                WindowOpen {
                    panes: vec![r("w1:p1")],
                    tab: Some(r("w1:t1")),
                    bounds: None
                },
                &mut alloc
            )
            .is_err()
    );
    assert!(
        layout
            .open_window(
                WindowOpen {
                    panes: vec![r("w9:p1")],
                    tab: None,
                    bounds: None
                },
                &mut alloc
            )
            .is_err()
    );
    layout.validate().expect("valid");
}

#[test]
fn a_window_follows_tab_select_set_visible_panes_and_bounds() {
    let (mut layout, mut alloc) = layout_with(1);
    layout
        .create_tab(
            TabCreate {
                workspace: r("w1"),
                name: None,
                command: None,
            },
            &mut alloc,
        )
        .expect("a tab");
    let window = layout
        .open_window(
            WindowOpen {
                panes: vec![],
                tab: None,
                bounds: None,
            },
            &mut alloc,
        )
        .expect("opened")
        .value
        .window;
    // The window shows the selected tab (t2). Select t1 in this window.
    layout
        .select_tab(&r("w1:t1"), Some(window))
        .expect("selected");
    let win = layout.windows[0].clone();
    assert_eq!(win.tab, Some(layout.workspaces[0].tabs[0].id));
    assert_eq!(
        win.by_workspace.get(&layout.workspaces[0].id),
        Some(&layout.workspaces[0].tabs[0].id)
    );
    // An unknown window is refused.
    let missing = WindowId::new(ulid::Ulid::from(9u128));
    assert!(matches!(
        layout.select_tab(&r("w1:t1"), Some(missing)),
        Err(LayoutError::NotFound(_))
    ));
    // Set the visible panes: the window shows no tab then.
    layout
        .set_visible_panes(window, &[r("w1:p2")])
        .expect("set");
    let win = layout.windows[0].clone();
    assert!(win.tab.is_none());
    assert_eq!(
        layout.visible_panes(&win),
        vec![layout.resolve_pane(&r("w1:p2")).expect("p2")]
    );
    layout
        .set_window_bounds(
            window,
            Bounds {
                x: 0,
                y: 0,
                width: 10,
                height: 10,
            },
        )
        .expect("bounds");
    assert_eq!(layout.windows[0].bounds.map(|b| b.width), Some(10));
    assert!(
        layout
            .set_window_bounds(
                missing,
                Bounds {
                    x: 0,
                    y: 0,
                    width: 1,
                    height: 1
                }
            )
            .is_err()
    );
    layout.validate().expect("valid");
}

#[test]
fn a_window_of_a_closed_tab_shows_another_tab_and_forgets_closed_panes() {
    let (mut layout, mut alloc) = layout_with(1);
    layout
        .create_tab(
            TabCreate {
                workspace: r("w1"),
                name: None,
                command: None,
            },
            &mut alloc,
        )
        .expect("a tab");
    let window = layout
        .open_window(
            WindowOpen {
                panes: vec![],
                tab: Some(r("w1:t2")),
                bounds: None,
            },
            &mut alloc,
        )
        .expect("opened")
        .value
        .window;
    layout.windows[0].focused_pane = Some(layout.resolve_pane(&r("w1:p2")).expect("p2"));
    layout.close_tab(&r("w1:t2"), false).expect("closed");
    let win = &layout.windows[0];
    assert_eq!(win.id, window);
    assert_eq!(
        win.tab,
        Some(layout.workspaces[0].tabs[0].id),
        "it falls back to a tab that is there"
    );
    assert_eq!(win.focused_pane, None, "the focused pane is gone");
    // Panes that close leave an explicit list.
    layout
        .set_visible_panes(window, &[r("w1:p1")])
        .expect("set");
    layout.close_pane(&close("w1:p1", false)).expect("closed");
    assert_eq!(layout.windows[0].explicit, Some(vec![]));
    layout.validate().expect("valid");
}

#[test]
fn pane_values_come_from_the_layout() {
    let (mut layout, mut alloc) = layout_with(2);
    split(&mut layout, &mut alloc, "w1:p1", SplitDirection::Right);
    let all = layout.pane_values(None, None).expect("values");
    assert_eq!(all.len(), 3);
    let in_w1 = layout.pane_values(Some(&r("w1")), None).expect("values");
    assert_eq!(in_w1.len(), 2);
    let in_tab = layout.pane_values(None, Some(&r("w2:t1"))).expect("values");
    assert_eq!(in_tab.len(), 1);
    assert_eq!(in_tab[0].handle, "w2:p1");
    assert!(layout.pane_values(Some(&r("w9")), None).is_err());
    let p = layout.resolve_pane(&r("w1:p1")).expect("p1");
    assert_eq!(
        layout.pane_value(&layout.panes[&p]).liveness,
        PaneLiveness::Starting
    );
    layout.panes.get_mut(&p).expect("a pane").runtime = Runtime::Exited(Some(3));
    assert_eq!(
        layout.pane_value(&layout.panes[&p]).liveness,
        PaneLiveness::Exited { code: Some(3) }
    );
}

// --- a property over all the commands --------------------------------------------------------

#[derive(Clone, Debug)]
enum Op {
    Split(usize, u8),
    Close(usize),
    Move(usize, usize, Option<usize>, u8),
    Float(usize),
    Unfloat(usize),
    Popout(usize),
    CloseWindow(usize),
    NewTab(usize),
    CloseTab(usize),
    NewWorkspace,
    CloseWorkspace(usize),
    OpenWindow(usize),
    SelectTab(usize, usize),
    Zoom(usize, usize),
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        4 => (any::<usize>(), any::<u8>()).prop_map(|(a, b)| Op::Split(a, b)),
        3 => any::<usize>().prop_map(Op::Close),
        3 => (any::<usize>(), any::<usize>(), prop::option::of(any::<usize>()), any::<u8>())
            .prop_map(|(a, b, c, d)| Op::Move(a, b, c, d)),
        2 => any::<usize>().prop_map(Op::Float),
        2 => any::<usize>().prop_map(Op::Unfloat),
        2 => any::<usize>().prop_map(Op::Popout),
        2 => any::<usize>().prop_map(Op::CloseWindow),
        2 => any::<usize>().prop_map(Op::NewTab),
        1 => any::<usize>().prop_map(Op::CloseTab),
        1 => Just(Op::NewWorkspace),
        1 => any::<usize>().prop_map(Op::CloseWorkspace),
        2 => any::<usize>().prop_map(Op::OpenWindow),
        2 => (any::<usize>(), any::<usize>()).prop_map(|(a, b)| Op::SelectTab(a, b)),
        1 => (any::<usize>(), any::<usize>()).prop_map(|(a, b)| Op::Zoom(a, b)),
    ]
}

fn direction(n: u8) -> SplitDirection {
    [
        SplitDirection::Left,
        SplitDirection::Right,
        SplitDirection::Up,
        SplitDirection::Down,
    ][usize::from(n % 4)]
}

fn pick<T: Clone>(items: &[T], index: usize) -> Option<T> {
    (!items.is_empty()).then(|| items[index % items.len()].clone())
}

/// Applies an operation to a copy, as a handler does. The copy replaces the layout only when the
/// command works.
fn apply(layout: &mut Layout, alloc: &mut Counters, op: &Op) -> Result<(), LayoutError> {
    let mut next = layout.clone();
    let panes: Vec<PaneId> = next.panes.keys().copied().collect();
    let tabs: Vec<TabId> = next
        .workspaces
        .iter()
        .flat_map(|w| w.tabs.iter().map(|t| t.id))
        .collect();
    let windows: Vec<WindowId> = next.windows.iter().map(|w| w.id).collect();
    let workspaces: Vec<_> = next.workspaces.iter().map(|w| w.id).collect();
    let result: Result<(), LayoutError> = match op {
        Op::Split(a, d) => pick(&panes, *a).map_or(Ok(()), |p| {
            next.split_pane(
                PaneSplit {
                    pane: Ref::id(p),
                    direction: direction(*d),
                    ratio: None,
                    command: None,
                },
                alloc,
            )
            .map(|_| ())
        }),
        Op::Close(a) => pick(&panes, *a).map_or(Ok(()), |p| {
            next.close_pane(&PaneClose {
                pane: Ref::id(p),
                force: true,
            })
            .map(|_| ())
        }),
        Op::Move(a, b, beside, d) => match (pick(&panes, *a), pick(&tabs, *b)) {
            (Some(p), Some(t)) => next
                .move_pane(&PaneMove {
                    pane: Ref::id(p),
                    tab: Ref::id(t),
                    beside: beside.and_then(|i| pick(&panes, i)).map(Ref::id),
                    direction: Some(direction(*d)),
                })
                .map(|_| ()),
            _ => Ok(()),
        },
        Op::Float(a) => {
            pick(&panes, *a).map_or(Ok(()), |p| next.float_pane(&Ref::id(p), None).map(|_| ()))
        }
        Op::Unfloat(a) => {
            pick(&panes, *a).map_or(Ok(()), |p| next.unfloat_pane(&Ref::id(p)).map(|_| ()))
        }
        Op::Popout(a) => {
            pick(&panes, *a).map_or(Ok(()), |p| next.popout_pane(&Ref::id(p), alloc).map(|_| ()))
        }
        Op::CloseWindow(a) => {
            pick(&windows, *a).map_or(Ok(()), |w| next.close_window(w).map(|_| ()))
        }
        Op::NewTab(a) => pick(&workspaces, *a).map_or(Ok(()), |w| {
            next.create_tab(
                TabCreate {
                    workspace: Ref::id(w),
                    name: None,
                    command: None,
                },
                alloc,
            )
            .map(|_| ())
        }),
        Op::CloseTab(a) => {
            pick(&tabs, *a).map_or(Ok(()), |t| next.close_tab(&Ref::id(t), true).map(|_| ()))
        }
        Op::NewWorkspace => next.create_workspace(ws("W"), alloc).map(|_| ()),
        Op::CloseWorkspace(a) => pick(&workspaces, *a).map_or(Ok(()), |w| {
            next.close_workspace(&Ref::id(w), true).map(|_| ())
        }),
        Op::OpenWindow(a) => {
            let tab = pick(&tabs, *a).map(Ref::id);
            next.open_window(
                WindowOpen {
                    panes: vec![],
                    tab,
                    bounds: None,
                },
                alloc,
            )
            .map(|_| ())
        }
        Op::SelectTab(a, b) => match (pick(&tabs, *a), pick(&windows, *b)) {
            (Some(t), w) => next.select_tab(&Ref::id(t), w).map(|_| ()),
            _ => Ok(()),
        },
        Op::Zoom(a, b) => match (pick(&tabs, *a), pick(&panes, *b)) {
            (Some(t), p) => next.zoom(&Ref::id(t), p.map(Ref::id).as_ref()).map(|_| ()),
            _ => Ok(()),
        },
    };
    if result.is_ok() {
        *layout = next;
    }
    result
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    /// After any sequence of commands the layout keeps every rule: each pane is in one place,
    /// every tab keeps a pane in its tree, handles and render ids are unique, windows point to
    /// things that exist. A command that fails is an error of the caller, never a bug of the core.
    #[test]
    fn any_commands_keep_the_layout_valid(ops in prop::collection::vec(op(), 1..80)) {
        let (mut layout, mut alloc) = layout_with(2);
        for op in ops {
            match apply(&mut layout, &mut alloc, &op) {
                Ok(()) => {}
                Err(LayoutError::Internal(message)) => {
                    prop_assert!(false, "an internal error after {op:?}: {message}");
                }
                Err(_) => {}
            }
            prop_assert_eq!(layout.validate(), Ok(()), "after {:?}", op);
        }
        // Handles of the panes that are left were handed out once. The counters only go up.
        let mut handles: Vec<_> = layout.panes.values().map(|p| p.handle.clone()).collect();
        let total = handles.len();
        handles.sort();
        handles.dedup();
        prop_assert_eq!(handles.len(), total);
    }
}

fn report(runtime: Runtime) -> Report {
    Report {
        runtime,
        cols: 100,
        rows: 30,
        facts: Facts {
            title: Some("vim".to_owned()),
            restoring: false,
            input_unconfirmed: vec![7],
            clipboard_request: None,
            clipboard_dropped: None,
            geometry_owner: None,
            cwd: None,
        },
    }
}

#[test]
fn a_report_changes_the_pane_and_the_end_of_the_child_gives_one_event() {
    let (mut layout, _) = layout_with(1);
    let pane = layout.resolve_pane(&r("w1:p1")).expect("p1");
    // What the layout has already is no change.
    let same = Report {
        runtime: Runtime::Created,
        cols: DEFAULT_SIZE.0,
        rows: DEFAULT_SIZE.1,
        facts: Facts::default(),
    };
    assert!(!layout.apply_report(pane, same).changed);

    let running = layout.apply_report(pane, report(Runtime::Running));
    assert!(running.changed);
    assert!(running.event.is_none());
    let value = layout.pane_value(&layout.panes[&pane]);
    // The first report of a holder does not replace the size that the layout wants.
    assert_eq!((value.cols, value.rows), DEFAULT_SIZE);
    assert_eq!(value.title, "vim");
    assert_eq!(value.input_unconfirmed, [7]);
    assert_eq!(value.liveness, PaneLiveness::Alive);
    // The next report has the size of the PTY.
    assert!(layout.apply_report(pane, report(Runtime::Running)).changed);
    let value = layout.pane_value(&layout.panes[&pane]);
    assert_eq!((value.cols, value.rows), (100, 30));

    let exited = layout.apply_report(pane, report(Runtime::Exited(Some(3))));
    assert!(exited.changed);
    assert_eq!(
        kinds(exited.event.as_slice()),
        ["pane.exited"],
        "the child ended"
    );
    // The same report again changes nothing, so the event comes once.
    assert!(
        !layout
            .apply_report(pane, report(Runtime::Exited(Some(3))))
            .changed
    );
    // A holder that is lost after the end of the child is no second end.
    let lost = layout.apply_report(pane, report(Runtime::Lost));
    assert!(lost.changed);
    assert!(lost.event.is_none());
}

#[test]
fn a_report_about_a_pane_that_is_gone_changes_nothing() {
    let (mut layout, _) = layout_with(1);
    let before = layout.clone();
    let gone = PaneId::new(ulid::Ulid::from(999u128));
    let applied = layout.apply_report(gone, report(Runtime::Running));
    assert!(!applied.changed);
    assert!(applied.event.is_none());
    assert_eq!(layout, before);
}

#[test]
fn a_pane_that_ended_goes_back_to_created_with_the_command_it_is_given() {
    let (mut layout, _) = layout_with(1);
    let pane = layout.resolve_pane(&r("w1:p1")).expect("p1");
    // A pane that starts or runs does not restart.
    assert!(layout.restart_pane(&r("w1:p1"), None).is_err());
    layout.apply_report(pane, report(Runtime::Running));
    assert!(layout.restart_pane(&r("w1:p1"), None).is_err());

    layout.apply_report(pane, report(Runtime::Exited(Some(1))));
    let command = ProcessSpec {
        argv: vec!["vim".to_owned()],
        ..ProcessSpec::default()
    };
    let done = layout
        .restart_pane(&r("w1:p1"), Some(command))
        .expect("a restart");
    assert_eq!(kinds(&done.events), ["pane.restarted"]);
    let entry = &layout.panes[&pane];
    assert_eq!(entry.runtime, Runtime::Created);
    assert_eq!(entry.facts, Facts::default());
    assert_eq!(entry.cwd, None);
    assert_eq!(
        entry.spec.as_ref().map(|s| s.argv.clone()),
        Some(vec!["vim".to_owned()])
    );
    layout.validate().expect("valid");

    // A holder that was lost restarts too. Without a command the pane keeps its program.
    layout.apply_report(pane, report(Runtime::Lost));
    layout.restart_pane(&r("w1:p1"), None).expect("a restart");
    assert_eq!(
        layout.panes[&pane].spec.as_ref().map(|s| s.argv.clone()),
        Some(vec!["vim".to_owned()])
    );
}

#[test]
fn a_size_asked_for_before_the_holder_runs_is_noted_and_kept() {
    let (mut layout, _) = layout_with(1);
    let pane = layout.resolve_pane(&r("w1:p1")).expect("p1");
    assert!(layout.note_size(pane, 132, 43));
    assert!(
        !layout.note_size(pane, 132, 43),
        "the same size changes nothing"
    );
    // The first report of the holder says the size it started with, and the wanted size stays.
    layout.apply_report(pane, report(Runtime::Running));
    assert_eq!(
        (layout.panes[&pane].cols, layout.panes[&pane].rows),
        (132, 43)
    );
    // A pane that runs takes its size from the holder, not from the notes of windows.
    assert!(!layout.note_size(pane, 10, 10));
    // A pane that is gone has no size.
    assert!(!layout.note_size(PaneId::new(ulid::Ulid::from(999u128)), 10, 10));
}

#[test]
fn a_window_that_shows_nothing_takes_the_first_workspace_that_appears() {
    let mut layout = Layout::default();
    let mut alloc = Counters::new();
    // The first window of a first launch opens before any workspace exists.
    let window = layout
        .open_window(
            WindowOpen {
                panes: Vec::new(),
                tab: None,
                bounds: None,
            },
            &mut alloc,
        )
        .expect("a window")
        .value
        .window;
    assert_eq!(layout.windows[0].tab, None);

    let created = layout
        .create_workspace(ws("A"), &mut alloc)
        .expect("a workspace")
        .value;
    assert_eq!(layout.windows[0].tab, Some(created.tab));
    let shown = layout.visible_panes(&layout.windows[0]);
    assert_eq!(shown, [created.pane]);
    assert_eq!(layout.windows[0].id, window);

    // A second workspace does not take the window from the first one.
    layout
        .create_workspace(ws("B"), &mut alloc)
        .expect("a workspace");
    assert_eq!(layout.windows[0].tab, Some(created.tab));
    layout.validate().expect("valid");
}

#[test]
fn a_single_window_draws_its_focused_pane_or_the_first_pane_of_its_tab() {
    let (mut layout, mut alloc) = layout_with(1);
    let second = split(&mut layout, &mut alloc, "w1:p1", SplitDirection::Right);
    let third = split(&mut layout, &mut alloc, "w1:p2", SplitDirection::Down);
    layout.float_pane(&r("w1:p3"), None).expect("floated");
    let open = |layout: &mut Layout, alloc: &mut Counters| {
        layout
            .open_window(
                WindowOpen {
                    panes: vec![],
                    tab: None,
                    bounds: None,
                },
                alloc,
            )
            .expect("opened")
            .value
            .window
    };
    let phone = open(&mut layout, &mut alloc);
    let mac = open(&mut layout, &mut alloc);
    let first = layout.resolve_pane(&r("w1:p1")).expect("a pane");
    let window = |layout: &Layout, id| {
        layout
            .windows
            .iter()
            .find(|w| w.id == id)
            .expect("the window")
            .clone()
    };
    layout.set_window_single(phone, true);

    // No pane has the focus: the first pane of the tab.
    assert_eq!(layout.visible_panes(&window(&layout, phone)), [first]);
    // Every pane of the tab can get the focus, and the focus chooses the pane that it draws.
    let target = layout
        .focus_target(phone, &r("w1:p2"))
        .expect("a pane of the tab");
    layout.set_focus(phone, target);
    assert_eq!(layout.visible_panes(&window(&layout, phone)), [second]);
    // A floating pane is one more pane of the tab.
    let target = layout
        .focus_target(phone, &r("w1:p3"))
        .expect("a floating pane");
    layout.set_focus(phone, target);
    assert_eq!(layout.visible_panes(&window(&layout, phone)), [third]);
    // The window of the Mac shows the same tab and keeps all of its panes.
    assert_eq!(layout.visible_panes(&window(&layout, mac)).len(), 3);
    assert!(layout.focus_target(mac, &r("w1:p3")).is_ok());
    // Without `single`, the phone draws the tab again.
    layout.set_window_single(phone, false);
    assert_eq!(layout.visible_panes(&window(&layout, phone)).len(), 3);
    assert!(!layout.window_value(&window(&layout, phone)).single);
}
