use nh_proto::api::{
    calls::workspace::{TabCreate, WorkspaceCreate},
    common::ProcessSpec,
    events::EventKind,
    ids::{GroupId, PaneId, TabId, WorkspaceId},
    model::LayoutNode,
};

use super::*;
use crate::layout::testing::{Counters, r};

fn ws_params(name: &str) -> WorkspaceCreate {
    WorkspaceCreate {
        name: Some(name.to_owned()),
        root: None,
        group: None,
        machine: None,
        command: None,
    }
}

fn kinds(events: &[NewEvent]) -> Vec<&'static str> {
    events.iter().map(|e| e.payload.kind().name()).collect()
}

/// A layout with `n` workspaces, made by the commands, and the counters that made it.
fn layout_with(n: usize) -> (Layout, Counters) {
    let mut layout = Layout::default();
    let mut alloc = Counters::new();
    for i in 1..=n {
        layout
            .create_workspace(ws_params(&format!("Work {i}")), &mut alloc)
            .expect("a workspace");
    }
    layout.validate().expect("a valid layout");
    (layout, alloc)
}

#[test]
fn a_workspace_starts_with_one_tab_that_has_one_pane() {
    let mut layout = Layout::default();
    let mut alloc = Counters::new();
    let done = layout
        .create_workspace(ws_params("Main"), &mut alloc)
        .expect("created");
    let created = done.value;
    assert_eq!(created.handle, "w1");
    assert_eq!(created.tab_handle, "w1:t1");
    assert_eq!(created.pane_handle, "w1:p1");
    assert_eq!(
        kinds(&done.events),
        ["workspace.created", "tab.created", "pane.created"]
    );
    let ws = &layout.workspaces[0];
    assert_eq!(
        (ws.name.as_str(), ws.number, ws.machine.as_str()),
        ("Main", 1, "local")
    );
    assert_eq!(ws.tabs.len(), 1);
    assert_eq!(ws.selected_tab, Some(created.tab));
    assert_eq!(ws.tabs[0].layout, LayoutNode::Pane { pane: created.pane });
    let pane = &layout.panes[&created.pane];
    assert_eq!((pane.render_id, pane.cols, pane.rows), (1, 80, 24));
    assert_eq!(pane.runtime, Runtime::Created);
    layout.validate().expect("valid");
    // The events name their objects.
    assert_eq!(done.events[0].subject, created.workspace.to_string());
    assert_eq!(done.events[2].subject, created.pane.to_string());
}

#[test]
fn handles_and_render_ids_count_up_and_never_come_back() {
    let (mut layout, mut alloc) = layout_with(2);
    let handles: Vec<_> = layout.workspaces.iter().map(|w| w.handle.clone()).collect();
    assert_eq!(handles, ["w1", "w2"]);
    let t = layout
        .create_tab(
            TabCreate {
                workspace: r("w2"),
                name: Some("Logs".into()),
                command: None,
            },
            &mut alloc,
        )
        .expect("a tab");
    assert_eq!(
        (t.value.handle.as_str(), t.value.pane_handle.as_str()),
        ("w2:t2", "w2:p2")
    );
    // Close the workspace w1: its handle stays used.
    layout.close_workspace(&r("w1"), false).expect("closed");
    let again = layout
        .create_workspace(ws_params("New"), &mut alloc)
        .expect("created");
    assert_eq!(again.value.handle, "w3", "w1 is not used again");
    // The panes had the render ids 1, 2 and 3. The pane of the closed w1 had 1, and the new pane
    // gets 4: no number comes back.
    let mut renders: Vec<u32> = layout.panes.values().map(|p| p.render_id).collect();
    renders.sort_unstable();
    assert_eq!(renders, [2, 3, 4]);
    layout.validate().expect("valid");
}

#[test]
fn the_name_of_a_workspace_comes_from_the_root_when_it_is_not_given() {
    let mut layout = Layout::default();
    let mut alloc = Counters::new();
    let params = WorkspaceCreate {
        name: None,
        root: Some("/home/me/projects/neohar".to_owned()),
        group: None,
        machine: None,
        command: None,
    };
    layout
        .create_workspace(params, &mut alloc)
        .expect("created");
    assert_eq!(layout.workspaces[0].name, "neohar");
    let params = WorkspaceCreate {
        name: None,
        root: None,
        group: None,
        machine: None,
        command: None,
    };
    layout
        .create_workspace(params, &mut alloc)
        .expect("created");
    assert_eq!(layout.workspaces[1].name, "Workspace 2");
}

#[test]
fn the_first_pane_runs_what_was_asked_and_starts_in_the_root() {
    let mut layout = Layout::default();
    let mut alloc = Counters::new();
    let params = WorkspaceCreate {
        name: Some("Build".into()),
        root: Some("/work/app".into()),
        group: None,
        machine: None,
        command: Some(ProcessSpec {
            argv: vec!["cargo".into(), "watch".into()],
            cwd: None,
            env: Default::default(),
        }),
    };
    let created = layout
        .create_workspace(params, &mut alloc)
        .expect("created")
        .value;
    let pane = &layout.panes[&created.pane];
    let spec = pane.spec.as_ref().expect("a spec");
    assert_eq!(spec.argv, ["cargo", "watch"]);
    assert_eq!(
        spec.cwd.as_deref(),
        Some("/work/app"),
        "the root is the cwd"
    );
    assert_eq!(pane.cwd, None, "no holder runs yet");
    // A workspace without a root and without a command: the login shell, nothing to say.
    let plain = layout
        .create_workspace(ws_params("Plain"), &mut alloc)
        .expect("created")
        .value;
    assert!(layout.panes[&plain.pane].spec.is_none());
    // With a root and no command: the shell in the root.
    let params = WorkspaceCreate {
        name: None,
        root: Some("/x".into()),
        group: None,
        machine: None,
        command: None,
    };
    let rooted = layout
        .create_workspace(params, &mut alloc)
        .expect("created")
        .value;
    let spec = layout.panes[&rooted.pane].spec.as_ref().expect("a spec");
    assert!(spec.argv.is_empty());
    assert_eq!(spec.cwd.as_deref(), Some("/x"));
}

#[test]
fn only_the_local_machine_exists_and_names_are_checked() {
    let mut layout = Layout::default();
    let mut alloc = Counters::new();
    let mut params = ws_params("Remote");
    params.machine = Some("devbox".into());
    assert!(matches!(
        layout.create_workspace(params, &mut alloc),
        Err(LayoutError::Unsupported(_))
    ));
    let mut params = ws_params("Here");
    params.machine = Some("local".into());
    assert!(layout.create_workspace(params, &mut alloc).is_ok());
    for bad in ["", "   ", "line\nbreak", &"x".repeat(NAME_MAX + 1)] {
        assert!(
            matches!(
                layout.create_workspace(ws_params(bad), &mut alloc),
                Err(LayoutError::Invalid(_))
            ),
            "{bad:?}"
        );
    }
    assert_eq!(
        layout.workspaces.len(),
        1,
        "a refused command may leave the layout half done, but the handler discards it"
    );
    assert_eq!(clean_name("  A name  ").expect("trimmed"), "A name");
    assert_eq!(
        clean_name(&"é".repeat(NAME_MAX))
            .expect("100 characters")
            .chars()
            .count(),
        NAME_MAX
    );
}

#[test]
fn a_workspace_is_found_by_id_or_handle_and_a_missing_one_is_not_found() {
    let (layout, _) = layout_with(2);
    let id = layout.workspaces[1].id;
    assert_eq!(layout.resolve_workspace(&Ref::id(id)).expect("by id"), 1);
    assert_eq!(layout.resolve_workspace(&r("w2")).expect("by handle"), 1);
    assert_eq!(
        layout
            .resolve_workspace(&r("local/w2"))
            .expect("with the local label"),
        1
    );
    for missing in ["w9", "devbox/w1"] {
        assert!(
            matches!(
                layout.resolve_workspace(&r(missing)),
                Err(LayoutError::NotFound(_))
            ),
            "{missing}"
        );
    }
    assert!(
        layout
            .resolve_workspace(&Ref::<WorkspaceId>::id(WorkspaceId::new(ulid::Ulid::from(
                7u128
            ))))
            .is_err()
    );
}

#[test]
fn rename_and_move_a_workspace() {
    let (mut layout, mut alloc) = layout_with(3);
    let done = layout
        .rename_workspace(&r("w2"), "  Docs ")
        .expect("renamed");
    assert_eq!(layout.workspaces[1].name, "Docs");
    assert_eq!(kinds(&done.events), ["workspace.renamed"]);
    assert!(layout.rename_workspace(&r("w2"), "").is_err());

    // w3 before w1.
    let done = layout
        .move_workspace(&r("w3"), None, Some(&r("w1")))
        .expect("moved");
    assert_eq!(kinds(&done.events), ["workspace.moved"]);
    let order: Vec<_> = layout
        .workspaces
        .iter()
        .map(|w| w.handle.as_str())
        .collect();
    assert_eq!(order, ["w3", "w1", "w2"]);
    // w3 to the end.
    layout.move_workspace(&r("w3"), None, None).expect("moved");
    let order: Vec<_> = layout
        .workspaces
        .iter()
        .map(|w| w.handle.as_str())
        .collect();
    assert_eq!(order, ["w1", "w2", "w3"]);
    // w1 before w3: it goes down by one place, and the target keeps its place.
    layout
        .move_workspace(&r("w1"), None, Some(&r("w3")))
        .expect("moved");
    let order: Vec<_> = layout
        .workspaces
        .iter()
        .map(|w| w.handle.as_str())
        .collect();
    assert_eq!(order, ["w2", "w1", "w3"]);
    assert!(
        layout
            .move_workspace(&r("w1"), None, Some(&r("w1")))
            .is_err()
    );

    // Groups: a move puts a workspace in a group, and without a group it leaves the group.
    let g = layout
        .create_group("Work", &mut alloc)
        .expect("a group")
        .value;
    layout
        .move_workspace(&r("w1"), Some(&Ref::id(g)), None)
        .expect("in a group");
    assert_eq!(
        layout
            .workspaces
            .iter()
            .find(|w| w.handle == "w1")
            .expect("w1")
            .group,
        Some(g)
    );
    layout
        .move_workspace(&r("w1"), None, None)
        .expect("out of the group");
    assert_eq!(
        layout
            .workspaces
            .iter()
            .find(|w| w.handle == "w1")
            .expect("w1")
            .group,
        None
    );
    layout.validate().expect("valid");
}

#[test]
fn groups_can_be_made_renamed_moved_and_closed() {
    let (mut layout, mut alloc) = layout_with(2);
    let a = layout.create_group("A", &mut alloc).expect("a group");
    assert_eq!(kinds(&a.events), ["group.created"]);
    let a = a.value;
    let b = layout.create_group("B", &mut alloc).expect("a group").value;
    layout.rename_group(&Ref::id(a), "Alpha").expect("renamed");
    assert_eq!(layout.groups[0].name, "Alpha");
    layout
        .move_group(&Ref::id(b), Some(&Ref::id(a)))
        .expect("moved");
    let names: Vec<_> = layout.groups.iter().map(|g| g.name.as_str()).collect();
    assert_eq!(names, ["B", "Alpha"]);
    assert!(layout.move_group(&Ref::id(a), Some(&Ref::id(a))).is_err());
    let params = WorkspaceCreate {
        group: Some(Ref::id(a)),
        ..ws_params("In A")
    };
    let ws = layout
        .create_workspace(params, &mut alloc)
        .expect("created")
        .value;
    let summaries = layout.group_summaries();
    assert_eq!(summaries[1].workspaces, [ws.workspace]);
    // Closing a group keeps the workspaces, with no group.
    let done = layout.close_group(&Ref::id(a)).expect("closed");
    assert_eq!(kinds(&done.events), ["group.closed"]);
    assert_eq!(layout.workspaces.len(), 3);
    assert!(layout.workspaces.iter().all(|w| w.group.is_none()));
    assert!(layout.resolve_group(&Ref::id(a)).is_err());
    // An unknown group is not found; a group name is not a reference.
    assert!(matches!(
        layout.rename_group(&Ref::<GroupId>::id(a), "x"),
        Err(LayoutError::NotFound(_))
    ));
    layout.validate().expect("valid");
}

#[test]
fn a_workspace_can_be_made_in_a_group_that_must_exist() {
    let mut layout = Layout::default();
    let mut alloc = Counters::new();
    let params = WorkspaceCreate {
        group: Some(Ref::id(GroupId::new(ulid::Ulid::from(5u128)))),
        ..ws_params("Lost")
    };
    assert!(matches!(
        layout.create_workspace(params, &mut alloc),
        Err(LayoutError::NotFound(_))
    ));
}

#[test]
fn a_tab_is_made_selected_renamed_and_closed() {
    let (mut layout, mut alloc) = layout_with(1);
    let first_tab = layout.workspaces[0].tabs[0].id;
    let made = layout
        .create_tab(
            TabCreate {
                workspace: r("w1"),
                name: None,
                command: None,
            },
            &mut alloc,
        )
        .expect("a tab");
    assert_eq!(kinds(&made.events), ["tab.created", "pane.created"]);
    let second = made.value.tab;
    assert_eq!(layout.workspaces[0].tabs[1].name, "Tab 2");
    assert_eq!(
        layout.workspaces[0].selected_tab,
        Some(second),
        "a new tab is selected"
    );
    layout
        .select_tab(&Ref::id(first_tab), None)
        .expect("selected");
    assert_eq!(layout.workspaces[0].selected_tab, Some(first_tab));
    layout.rename_tab(&r("w1:t2"), "Logs").expect("renamed");
    assert_eq!(layout.workspaces[0].tabs[1].name, "Logs");

    // Closing the selected tab selects the one before it. The panes of the tab go.
    layout.select_tab(&Ref::id(second), None).expect("selected");
    let done = layout.close_tab(&Ref::id(second), false).expect("closed");
    assert_eq!(kinds(&done.events), ["pane.closed", "tab.closed"]);
    assert_eq!(layout.workspaces[0].selected_tab, Some(first_tab));
    assert_eq!(layout.panes.len(), 1);
    // The last tab can close too. The workspace stays, with no tab.
    layout
        .close_tab(&Ref::id(first_tab), false)
        .expect("closed");
    assert!(layout.workspaces[0].tabs.is_empty());
    assert_eq!(layout.workspaces[0].selected_tab, None);
    assert!(layout.panes.is_empty());
    layout.validate().expect("valid");
    // A tab can be made again in an empty workspace.
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
    assert_eq!(
        layout.workspaces[0].tabs[0].handle, "w1:t3",
        "t1 and t2 are not used again"
    );
    layout.validate().expect("valid");
}

#[test]
fn tabs_move_inside_a_workspace_and_to_another_one() {
    let (mut layout, mut alloc) = layout_with(2);
    for _ in 0..2 {
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
    }
    let order = |layout: &Layout| -> Vec<String> {
        layout.workspaces[0]
            .tabs
            .iter()
            .map(|t| t.handle.clone())
            .collect()
    };
    assert_eq!(order(&layout), ["w1:t1", "w1:t2", "w1:t3"]);
    layout.move_tab(&r("w1:t3"), None, Some(0)).expect("moved");
    assert_eq!(order(&layout), ["w1:t3", "w1:t1", "w1:t2"]);
    layout
        .move_tab(&r("w1:t3"), None, Some(99))
        .expect("moved to the end");
    assert_eq!(order(&layout), ["w1:t1", "w1:t2", "w1:t3"]);

    // To another workspace: the panes follow, and they keep their handles.
    let tab = layout.workspaces[0].tabs[1].id;
    let pane = layout.panes_of_tab(tab)[0].id;
    let handle = layout.panes[&pane].handle.clone();
    let done = layout
        .move_tab(&r("w1:t2"), Some(&r("w2")), Some(0))
        .expect("moved");
    assert_eq!(kinds(&done.events), ["tab.moved"]);
    assert_eq!(layout.workspaces[0].tabs.len(), 2);
    assert_eq!(layout.workspaces[1].tabs[0].id, tab);
    assert_eq!(layout.panes[&pane].workspace, layout.workspaces[1].id);
    assert_eq!(layout.panes[&pane].handle, handle, "a handle never changes");
    layout.validate().expect("valid");
    // The tab is found by its handle still, in its new workspace.
    assert_eq!(layout.resolve_tab(&r("w1:t2")).expect("found"), (1, 0));
}

#[test]
fn moving_the_selected_tab_away_selects_another_one_in_the_old_workspace() {
    let (mut layout, mut alloc) = layout_with(2);
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
    assert_eq!(
        layout.workspaces[0].selected_tab,
        Some(layout.workspaces[0].tabs[1].id)
    );
    layout
        .move_tab(&r("w1:t2"), Some(&r("w2")), None)
        .expect("moved");
    assert_eq!(
        layout.workspaces[0].selected_tab,
        Some(layout.workspaces[0].tabs[0].id)
    );
    // Moving the only tab out leaves nothing selected.
    layout
        .move_tab(&r("w1:t1"), Some(&r("w2")), None)
        .expect("moved");
    assert_eq!(layout.workspaces[0].selected_tab, None);
    layout.validate().expect("valid");
}

#[test]
fn a_live_pane_needs_force_to_close() {
    let (mut layout, mut alloc) = layout_with(1);
    let tab = layout.workspaces[0].tabs[0].id;
    let pane = layout.panes_of_tab(tab)[0].id;
    layout.panes.get_mut(&pane).expect("the pane").runtime = Runtime::Running;
    let before = layout.clone();
    for refused in [
        layout.clone().close_tab(&Ref::id(tab), false).map(|_| ()),
        layout.clone().close_workspace(&r("w1"), false).map(|_| ()),
    ] {
        assert!(
            matches!(
                refused,
                Err(LayoutError::Confirm {
                    process: None,
                    is_agent: false
                })
            ),
            "{refused:?}"
        );
    }
    assert_eq!(layout, before);
    layout.close_tab(&Ref::id(tab), true).expect("forced");
    assert!(layout.panes.is_empty());
    // A pane that exited does not need force.
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
    let pane = *layout.panes.keys().next().expect("a pane");
    layout.panes.get_mut(&pane).expect("the pane").runtime = Runtime::Exited(Some(0));
    layout
        .close_workspace(&r("w1"), false)
        .expect("closed without force");
}

#[test]
fn closing_a_workspace_reports_every_pane_and_tab_first() {
    let (mut layout, mut alloc) = layout_with(2);
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
    let done = layout.close_workspace(&r("w1"), false).expect("closed");
    assert_eq!(
        kinds(&done.events),
        [
            "pane.closed",
            "tab.closed",
            "pane.closed",
            "tab.closed",
            "workspace.closed"
        ]
    );
    assert_eq!(layout.workspaces.len(), 1);
    assert_eq!(layout.panes.len(), 1);
    layout.validate().expect("valid");
}

#[test]
fn ratios_equalize_and_zoom_work_on_a_tab() {
    let (mut layout, _) = layout_with(1);
    // A tab with a split: build it with the tree operations on the stored tab.
    let tab_id = layout.workspaces[0].tabs[0].id;
    let first = layout.panes_of_tab(tab_id)[0].id;
    let second = PaneId::new(ulid::Ulid::from(2_000u128));
    let third = PaneId::new(ulid::Ulid::from(2_001u128));
    {
        let tab = &mut layout.workspaces[0].tabs[0];
        tree::split(
            &mut tab.layout,
            first,
            second,
            nh_proto::api::common::SplitDirection::Right,
            None,
        )
        .expect("a split");
        tree::split(
            &mut tab.layout,
            second,
            third,
            nh_proto::api::common::SplitDirection::Down,
            None,
        )
        .expect("a split");
    }
    for (pane, n) in [(second, 2u32), (third, 3)] {
        layout.panes.insert(
            pane,
            PaneEntry {
                id: pane,
                handle: format!("w1:p{n}"),
                render_id: n,
                workspace: layout.workspaces[0].id,
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
    }
    layout.validate().expect("valid");

    let done = layout
        .set_ratios(&Ref::id(tab_id), &[], &[0.25, 0.75])
        .expect("ratios");
    assert_eq!(kinds(&done.events), ["layout.changed"]);
    let LayoutNode::Split { ratios, .. } = &layout.workspaces[0].tabs[0].layout else {
        panic!("a split")
    };
    assert_eq!(ratios, &[0.25, 0.75]);
    assert!(layout.set_ratios(&Ref::id(tab_id), &[], &[0.5]).is_err());
    assert!(
        layout
            .set_ratios(&Ref::id(tab_id), &[0], &[0.5, 0.5])
            .is_err(),
        "a pane has no ratios"
    );
    layout.equalize(&Ref::id(tab_id), &[]).expect("equalized");
    let LayoutNode::Split { ratios, .. } = &layout.workspaces[0].tabs[0].layout else {
        panic!("a split")
    };
    assert_eq!(ratios, &[0.5, 0.5]);

    // Zoom a pane of the tab, and end the zoom.
    layout
        .zoom(&Ref::id(tab_id), Some(&Ref::id(third)))
        .expect("zoomed");
    assert_eq!(layout.workspaces[0].tabs[0].zoomed, Some(third));
    layout.validate().expect("valid");
    layout.zoom(&Ref::id(tab_id), None).expect("zoom ended");
    assert_eq!(layout.workspaces[0].tabs[0].zoomed, None);
    // A pane of another tab, or one that does not exist, cannot be zoomed.
    let other = layout
        .create_tab(
            TabCreate {
                workspace: r("w1"),
                name: None,
                command: None,
            },
            &mut Counters::new(),
        )
        .expect("a tab")
        .value;
    assert!(matches!(
        layout.zoom(&Ref::id(tab_id), Some(&Ref::id(other.pane))),
        Err(LayoutError::Invalid(_))
    ));
    assert!(matches!(
        layout.zoom(
            &Ref::id(tab_id),
            Some(&Ref::id(PaneId::new(ulid::Ulid::from(9u128))))
        ),
        Err(LayoutError::NotFound(_))
    ));
}

#[test]
fn summaries_count_tabs_panes_and_agents() {
    let (mut layout, mut alloc) = layout_with(2);
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
    // One pane of `w1` has a live agent run.
    let with_agent = layout.panes_of_workspace(layout.workspaces[0].id)[0].id;
    let all = layout.workspace_summaries(None, &|pane| pane == with_agent);
    assert_eq!((all[0].tabs, all[0].panes, all[0].agents), (2, 2, 1));
    assert_eq!((all[1].tabs, all[1].panes, all[1].agents), (1, 1, 0));
    let g = layout.create_group("G", &mut alloc).expect("a group").value;
    layout
        .move_workspace(&r("w2"), Some(&Ref::id(g)), None)
        .expect("in a group");
    let in_group = layout.workspace_summaries(Some(g), &|_| false);
    assert_eq!(in_group.len(), 1);
    assert_eq!(in_group[0].handle, "w2");
    assert_eq!(in_group[0].group, Some(g));
}

#[test]
fn an_unknown_tab_is_not_found() {
    let (mut layout, _) = layout_with(1);
    for result in [
        layout.rename_tab(&r("w1:t9"), "x"),
        layout.close_tab(
            &Ref::<TabId>::id(TabId::new(ulid::Ulid::from(3u128))),
            false,
        ),
        layout.select_tab(&r("w9:t1"), None),
    ] {
        assert!(matches!(result, Err(LayoutError::NotFound(_))));
    }
}

#[test]
fn the_event_kinds_exist_in_the_contract() {
    for name in [
        "workspace.created",
        "workspace.renamed",
        "workspace.moved",
        "workspace.closed",
        "group.created",
        "group.renamed",
        "group.moved",
        "group.closed",
        "tab.created",
        "tab.renamed",
        "tab.moved",
        "tab.closed",
        "tab.selected",
        "pane.created",
        "pane.closed",
        "layout.changed",
    ] {
        assert!(EventKind::from_name(name).is_some(), "{name}");
    }
}
