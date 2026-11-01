//! `palette.query` and `palette.execute`.
//!
//! Query ranks the commands of the palette for a text. Execute runs one command on the layout, in
//! the transaction of the command, and it needs the window that runs it for the commands on "the
//! tab" or "the pane in focus". A command of the page (the inbox, the settings) does nothing in the
//! core. The result names it, and the page does it.

use nh_proto::api::{
    calls::{
        pane::{PaneClose, PaneSplit},
        system::{PaletteExecute, PaletteExecuted, PaletteResults},
        workspace::{TabCreate, WindowOpen, WorkspaceCreate},
    },
    common::SplitDirection,
    ids::{PaneId, Ref, TabId, WindowId, WorkspaceId},
    methods::{Call, Method},
};
use nh_store::Tx;
use serde_json::{Value, to_value};
use ulid::Ulid;

use super::{Ctx, Handled, HandlerTable, Mutation, State, layout::run};
use crate::{
    ErrorCode,
    error::RpcError,
    layout::{model::Layout, project::publish_window},
    palette::{self, Needs},
    projection::ProjectionHub,
};

/// The commands that a phone does not get: a phone has one window.
const NOT_ON_A_PHONE: [&str; 1] = ["window.new"];

/// How many results a query gives without a limit, and the most that it gives.
const DEFAULT_LIMIT: usize = 50;
const MAX_LIMIT: usize = 200;

fn palette_query(ctx: &Ctx<'_>, _: &mut Tx<'_>, call: &Call) -> Result<Handled, RpcError> {
    let Call::PaletteQuery(p) = call else {
        return Err(RpcError::internal("not palette.query"));
    };
    let limit = p
        .limit
        .map_or(DEFAULT_LIMIT, |n| usize::try_from(n).unwrap_or(MAX_LIMIT))
        .min(MAX_LIMIT);
    let degraded = ctx.state.degraded_reason();
    let mut items = palette::query(&ctx.state.layout, degraded.as_deref(), &p.query, limit);
    if ctx.origin.scope == super::Scope::Phone {
        items.retain(|item| !NOT_ON_A_PHONE.contains(&item.command.id.as_str()));
    }
    let result =
        to_value(PaletteResults { items }).map_err(|e| RpcError::internal(e.to_string()))?;
    Ok(Handled::result(result))
}

/// The result of a command that the core ran, and of one that the page runs.
fn executed(ui: Option<&str>) -> Result<Value, RpcError> {
    to_value(PaletteExecuted {
        ui: ui.map(str::to_owned),
    })
    .map_err(|e| RpcError::internal(e.to_string()))
}

/// The window that runs the command, as its index in the layout.
fn window_of(layout: &Layout, p: &PaletteExecute) -> Result<(WindowId, usize), RpcError> {
    let id = p
        .window
        .ok_or_else(|| RpcError::invalid_params("this command needs the window that runs it"))?;
    let index = layout
        .window_index(id)
        .ok_or_else(|| RpcError::new(ErrorCode::NotFound, format!("no window {id}")))?;
    Ok((id, index))
}

/// The tab that a window shows.
fn tab_of(layout: &Layout, window: usize) -> Result<(TabId, WorkspaceId), RpcError> {
    let tab = layout.windows[window]
        .tab
        .ok_or_else(|| RpcError::invalid_params("the window shows no tab"))?;
    let (w, _) = layout
        .tab_position(tab)
        .ok_or_else(|| RpcError::invalid_params("the window shows no tab"))?;
    Ok((tab, layout.workspaces[w].id))
}

/// The pane in focus in a window. When the focus names no pane that the window draws, the first
/// pane that it draws counts.
fn pane_of(layout: &Layout, window: usize) -> Result<PaneId, RpcError> {
    let shown = layout.visible_panes(&layout.windows[window]);
    layout.windows[window]
        .focused_pane
        .filter(|pane| shown.contains(pane))
        .or_else(|| shown.first().copied())
        .ok_or_else(|| RpcError::invalid_params("the window draws no pane"))
}

/// The id that follows a prefix, as an object of the layout.
fn parse_id<T: std::str::FromStr>(rest: &str) -> Result<T, RpcError> {
    rest.parse()
        .map_err(|_| RpcError::invalid_params(format!("{rest} is not an id")))
}

/// Gives the command the result of `palette.execute`.
fn done(handled: Handled) -> Result<Handled, RpcError> {
    Ok(Handled {
        result: executed(None)?,
        mutation: handled.mutation,
    })
}

fn palette_execute(ctx: &Ctx<'_>, tx: &mut Tx<'_>, call: &Call) -> Result<Handled, RpcError> {
    let Call::PaletteExecute(p) = call else {
        return Err(RpcError::internal("not palette.execute"));
    };
    let layout = &ctx.state.layout;

    // The commands that go to a workspace, a tab or a pane.
    if let Some(rest) = p.command.strip_prefix(palette::GO_WORKSPACE) {
        let (window, _) = window_of(layout, p)?;
        let id: Ulid = parse_id(rest)?;
        let index = layout
            .workspace_index(WorkspaceId::new(id))
            .ok_or_else(|| RpcError::new(ErrorCode::NotFound, format!("no workspace {id}")))?;
        let workspace = &layout.workspaces[index];
        let tab = workspace
            .selected_tab
            .or_else(|| workspace.tabs.first().map(|t| t.id))
            .ok_or_else(|| RpcError::invalid_params("the workspace has no tab"))?;
        return done(run(ctx, tx, |l, _| {
            l.select_tab(&Ref::<TabId>::id(tab), Some(window))
        })?);
    }
    if let Some(rest) = p.command.strip_prefix(palette::GO_TAB) {
        let (window, _) = window_of(layout, p)?;
        let id: Ulid = parse_id(rest)?;
        return done(run(ctx, tx, |l, _| {
            l.select_tab(&Ref::<TabId>::id(TabId::new(id)), Some(window))
        })?);
    }
    if let Some(rest) = p.command.strip_prefix(palette::GO_PANE) {
        let (window, _) = window_of(layout, p)?;
        let id: Ulid = parse_id(rest)?;
        let pane = PaneId::new(id);
        let entry = layout
            .panes
            .get(&pane)
            .ok_or_else(|| RpcError::new(ErrorCode::NotFound, format!("no pane {pane}")))?;
        let tab = entry.tab;
        let selected = run(ctx, tx, |l, _| {
            l.select_tab(&Ref::<TabId>::id(tab), Some(window))
        })?;
        // The tab first, and then the focus, which a window can give to a pane that it draws.
        let first = selected.mutation;
        let mutation: Mutation = Box::new(move |state: &mut State, hub: &ProjectionHub| {
            if let Some(first) = first {
                first(state, hub);
            }
            state.layout.set_focus(window, pane);
            publish_window(hub, &state.layout, window);
            if let Some(render) = &state.render {
                render.sync_layout(&state.layout);
            }
        });
        return Ok(Handled {
            result: executed(None)?,
            mutation: Some(mutation),
        });
    }

    let spec = palette::find(&p.command)
        .ok_or_else(|| RpcError::new(ErrorCode::NotFound, format!("no command {}", p.command)))?;
    if matches!(spec.needs, Needs::Page | Needs::PageTab) {
        return Ok(Handled::result(executed(Some(spec.id))?));
    }
    // A phone has no settings window: the page shows its settings view.
    if ctx.origin.scope == super::Scope::Phone {
        if matches!(spec.id, "settings.open" | "phone.pair") {
            return Ok(Handled::result(executed(Some("settings.open"))?));
        }
        if NOT_ON_A_PHONE.contains(&spec.id) {
            return Err(RpcError::new(
                ErrorCode::Unsupported,
                "A phone has one window.",
            ));
        }
    }
    match spec.id {
        // The settings window belongs to the app. The host opens it after the command is done.
        "settings.open" => {
            // The settings window opens on the client that asked.
            let device = ctx.origin.device;
            let open: Mutation = Box::new(move |state: &mut State, _: &ProjectionHub| {
                if let Some(host) = &state.host {
                    host.open_settings_for(device);
                }
            });
            Ok(Handled {
                result: executed(None)?,
                mutation: Some(open),
            })
        }
        // The section Phone of the settings window has the button "Pair a phone" and the dialog
        // of the QR code.
        "phone.pair" => {
            let device = ctx.origin.device;
            let open: Mutation = Box::new(move |state: &mut State, _: &ProjectionHub| {
                if let Some(host) = &state.host {
                    host.open_settings_at_for(device, "phone");
                }
            });
            Ok(Handled {
                result: executed(None)?,
                mutation: Some(open),
            })
        }
        "agent.resume_all" => {
            let control = ctx
                .state
                .control
                .clone()
                .ok_or_else(|| RpcError::internal("the core is not up yet"))?;
            let work: Mutation = Box::new(move |_: &mut State, _: &ProjectionHub| {
                let this = std::sync::Arc::clone(&control);
                control.background(
                    "Resume the agents",
                    async move { this.resume_offered().await },
                );
            });
            Ok(Handled {
                result: executed(None)?,
                mutation: Some(work),
            })
        }
        "workspace.new" => done(run(ctx, tx, |l, alloc| {
            l.create_workspace(
                WorkspaceCreate {
                    name: None,
                    root: None,
                    group: None,
                    machine: None,
                    command: None,
                },
                alloc,
            )
        })?),
        "window.new" => done(run(ctx, tx, |l, alloc| {
            l.open_window(
                WindowOpen {
                    panes: Vec::new(),
                    tab: None,
                    bounds: None,
                },
                alloc,
            )
        })?),
        id => run_on_window(ctx, tx, p, id),
    }
}

/// The commands on the tab or the pane of a window.
fn run_on_window(
    ctx: &Ctx<'_>,
    tx: &mut Tx<'_>,
    p: &PaletteExecute,
    id: &str,
) -> Result<Handled, RpcError> {
    let layout = &ctx.state.layout;
    let (window, index) = window_of(layout, p)?;
    let (tab, workspace) = tab_of(layout, index)?;
    // The page asks once, and then it calls again with `force` to stop the programs.
    let force = p
        .args
        .get("force")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let tab_ref = Ref::<TabId>::id(tab);
    let handled = match id {
        "tab.new" => run(ctx, tx, |l, alloc| {
            l.create_tab(
                TabCreate {
                    workspace: Ref::<WorkspaceId>::id(workspace),
                    name: None,
                    command: None,
                },
                alloc,
            )
        })?,
        "tab.close" => run(ctx, tx, |l, _| l.close_tab(&tab_ref, force))?,
        "workspace.close" => run(ctx, tx, |l, _| {
            l.close_workspace(&Ref::<WorkspaceId>::id(workspace), force)
        })?,
        "tab.next" | "tab.previous" => {
            let w = layout
                .workspace_index(workspace)
                .ok_or_else(|| RpcError::internal("a tab has no workspace"))?;
            let tabs = &layout.workspaces[w].tabs;
            let at = tabs.iter().position(|t| t.id == tab).unwrap_or(0);
            let step = if id == "tab.next" { 1 } else { tabs.len() - 1 };
            let next = tabs[(at + step) % tabs.len()].id;
            run(ctx, tx, |l, _| {
                l.select_tab(&Ref::<TabId>::id(next), Some(window))
            })?
        }
        _ => {
            let pane = pane_of(layout, index)?;
            let pane_ref = Ref::<PaneId>::id(pane);
            if id.starts_with("agent.") {
                return agent_command(ctx, id, pane);
            }
            match id {
                "pane.split_right" | "pane.split_down" => {
                    let direction = if id == "pane.split_right" {
                        SplitDirection::Right
                    } else {
                        SplitDirection::Down
                    };
                    run(ctx, tx, |l, alloc| {
                        l.split_pane(
                            PaneSplit {
                                pane: pane_ref,
                                direction,
                                ratio: None,
                                command: None,
                            },
                            alloc,
                        )
                    })?
                }
                "pane.close" => run(ctx, tx, |l, _| {
                    l.close_pane(&PaneClose {
                        pane: pane_ref,
                        force,
                    })
                })?,
                "pane.zoom" => {
                    let zoomed = layout
                        .tab_position(tab)
                        .and_then(|(w, t)| layout.workspaces[w].tabs[t].zoomed);
                    run(ctx, tx, |l, _| {
                        l.zoom(
                            &tab_ref,
                            if zoomed == Some(pane) {
                                None
                            } else {
                                Some(&pane_ref)
                            },
                        )
                    })?
                }
                "pane.float" => {
                    let floating = layout.tab_position(tab).is_some_and(|(w, t)| {
                        layout.workspaces[w].tabs[t]
                            .floating
                            .iter()
                            .any(|f| f.pane == pane)
                    });
                    run(ctx, tx, |l, _| {
                        if floating {
                            l.unfloat_pane(&pane_ref)
                        } else {
                            l.float_pane(&pane_ref, None)
                        }
                    })?
                }
                other => {
                    return Err(RpcError::new(
                        ErrorCode::NotFound,
                        format!("the command {other} has no action"),
                    ));
                }
            }
        }
    };
    done(handled)
}

/// The commands on the agent of the pane in focus. The work runs on the runtime, after the
/// command is done, and an error shows as a job that failed.
fn agent_command(ctx: &Ctx<'_>, id: &str, pane: PaneId) -> Result<Handled, RpcError> {
    let control = ctx
        .state
        .control
        .clone()
        .ok_or_else(|| RpcError::internal("the core is not up yet"))?;
    let run = ctx.state.runs.of_pane(pane).map(|run| run.id);
    let need_run =
        || run.ok_or_else(|| RpcError::new(ErrorCode::NotFound, "no agent runs in this pane"));
    let work: Mutation = match id {
        "agent.start_claude" => Box::new(move |_: &mut State, _: &ProjectionHub| {
            control.start_beside(nh_proto::api::common::Harness::Claude, pane);
        }),
        "agent.start_codex" => Box::new(move |_: &mut State, _: &ProjectionHub| {
            control.start_beside(nh_proto::api::common::Harness::Codex, pane);
        }),
        "agent.resume" => Box::new(move |_: &mut State, _: &ProjectionHub| {
            control.resume_in(pane);
        }),
        "agent.interrupt" => {
            let run = need_run()?;
            Box::new(move |_: &mut State, _: &ProjectionHub| {
                let this = std::sync::Arc::clone(&control);
                control.background("Interrupt the agent", async move {
                    this.interrupt(Ref::<nh_proto::api::ids::RunId>::id(run))
                        .await
                });
            })
        }
        "agent.stop" => {
            let run = need_run()?;
            Box::new(move |_: &mut State, _: &ProjectionHub| {
                let this = std::sync::Arc::clone(&control);
                control.background("Stop the agent", async move {
                    this.stop(Ref::<nh_proto::api::ids::RunId>::id(run), false)
                        .await
                });
            })
        }
        "agent.link" => {
            need_run()?;
            Box::new(move |_: &mut State, _: &ProjectionHub| {
                let this = std::sync::Arc::clone(&control);
                control.background("Link the agent", async move { this.link_pane(pane).await });
            })
        }
        other => {
            return Err(RpcError::new(
                ErrorCode::NotFound,
                format!("the command {other} has no action"),
            ));
        }
    };
    Ok(Handled {
        result: executed(None)?,
        mutation: Some(work),
    })
}

/// The handlers of this module.
pub(crate) fn handlers(table: &mut HandlerTable) {
    table.insert(Method::PaletteQuery, palette_query);
    table.insert(Method::PaletteExecute, palette_execute);
}
