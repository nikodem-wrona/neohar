//! The pane service: starts and ends the holders that the layout asks for.
//!
//! The layout decides which panes exist (`layout::launch`). A command that makes a pane leaves it in
//! the state `created`, and the state actor sends [`RuntimeCmd::Spawn`] after the commit. The
//! service starts a holder for the pane, gives it a pane token and the variables of neohar, and
//! puts the pane in the map of the core. From then on the pane task reports to the layout
//! through the actor. A closed pane gets [`RuntimeCmd::Close`], and the service kills its child
//! and every process that the child started.
//!
//! Every start runs in a task of its own, so that twenty new panes do not wait for each other.

use std::sync::{Arc, Weak};

use nh_proto::api::ids::PaneId;
use tokio::sync::mpsc;
use ulid::Ulid;

use super::PaneSpec;
use crate::{
    actor::layout::{apply_lost, size_of},
    error::CoreError,
    layout::launch::{Launch, RuntimeCmd},
    runtime::Inner,
};

/// Reads the commands of the layout until the core ends.
pub(crate) async fn run(inner: Weak<Inner>, mut commands: mpsc::UnboundedReceiver<RuntimeCmd>) {
    while let Some(command) = commands.recv().await {
        let Some(inner) = inner.upgrade() else {
            break;
        };
        match command {
            RuntimeCmd::Spawn(launch) if !inner.factory.tuning.start_holders => {
                inner.starting.finish(launch.pane.id.ulid());
            }
            RuntimeCmd::Spawn(launch) => {
                inner.runtime.clone().spawn(start_pane(inner, *launch));
            }
            RuntimeCmd::Close(pane) => close_pane(&inner, pane.ulid()),
        }
    }
}

/// Starts the holder of one pane and adds the pane to the core. The pane is in the map before it
/// leaves the set of the panes that start, so that nobody sees a gap between the two.
async fn start_pane(inner: Arc<Inner>, launch: Launch) {
    let id = launch.pane.id.ulid();
    match spawn(&inner, &launch).await {
        Ok(pane) => {
            inner.insert_pane(&pane);
            // A window may have asked for another size while the holder started.
            let wanted = inner
                .actor
                .update(move |parts| size_of(parts, PaneId::new(id)))
                .await
                .ok()
                .flatten();
            if let Some((cols, rows)) = wanted
                && pane.size() != (cols, rows)
                && let Err(error) = pane.resize(cols, rows, (0, 0), None)
            {
                tracing::debug!(pane = %id, %error, "the new pane is not sized");
            }
            if inner.starting.finish(id) {
                // The layout dropped the pane while its holder started. Nobody wants the process.
                inner.remove_pane(id);
                inner.tokens.revoke_pane(id);
                drop(inner.runtime.spawn_blocking(move || pane.kill_all()));
            }
        }
        Err(error) => {
            inner.tokens.revoke_pane(id);
            if !inner.starting.finish(id) {
                tracing::warn!(pane = %id, %error, "the holder of the pane does not start");
                // The pane shows that it has no process, and `pane.restart` tries again.
                inner
                    .actor
                    .update_detached(move |parts| apply_lost(parts, PaneId::new(id)));
            }
        }
    }
}

async fn spawn(inner: &Arc<Inner>, launch: &Launch) -> Result<Arc<super::Pane>, CoreError> {
    let id = launch.pane.id.ulid();
    let token = inner
        .tokens
        .issue(id)
        .map_err(|e| CoreError::Runtime(std::io::Error::other(e.message)))?;
    let mut spec = pane_spec(inner, launch, &token);
    // A window may have asked for another size after the layout asked for the holder.
    let pane_id = PaneId::new(id);
    if let Ok(Some((cols, rows))) = inner
        .actor
        .update(move |parts| size_of(parts, pane_id))
        .await
    {
        (spec.cols, spec.rows) = (cols, rows);
    }
    inner
        .factory
        .spawn_slot(id, launch.pane.render_id, spec, token)
        .await
}

/// The pane is gone from the layout: end its child, and forget the pane and its token.
fn close_pane(inner: &Arc<Inner>, pane: Ulid) {
    inner.tokens.revoke_pane(pane);
    if inner.starting.contains(pane) {
        // The link marked the pane as closed. The start ends the child when it finishes.
        return;
    }
    let removed = inner.remove_pane(pane);
    if let Some(removed) = removed {
        // The kill walks the processes of the pane, which blocks for a few milliseconds.
        drop(inner.runtime.spawn_blocking(move || removed.kill_all()));
    }
}

/// What the holder starts, with the variables of neohar. The login shell is the default.
fn pane_spec(inner: &Inner, launch: &Launch, token: &str) -> PaneSpec {
    let factory = &inner.factory;
    let spec = launch.pane.spec.clone().unwrap_or_default();
    let argv = if spec.argv.is_empty() {
        let shell = factory
            .base_env
            .iter()
            .find(|(key, _)| key == "SHELL")
            .map_or_else(|| "/bin/sh".to_owned(), |(_, value)| value.clone());
        vec![shell, "-l".to_owned()]
    } else {
        spec.argv
    };
    // The variables of the spec come first, so that they cannot replace the ones of neohar.
    let mut env: Vec<(String, String)> = spec.env.into_iter().collect();
    // The shims come first in the PATH. The PATH is the one the spec gives, or else the
    // one the pane starts from.
    if let Some(shims) = &factory.shims_dir
        && inner.config.config().agents.shims
    {
        let at = env.iter().position(|(key, _)| key == "PATH");
        let base = match at {
            Some(at) => env.remove(at).1,
            None => factory
                .base_env
                .iter()
                .find(|(key, _)| key == "PATH")
                .map(|(_, value)| value.clone())
                .unwrap_or_default(),
        };
        let path = if base.is_empty() {
            shims.display().to_string()
        } else {
            format!("{}:{base}", shims.display())
        };
        env.push(("PATH".to_owned(), path));
        env.push(("NEOHAR_SHIMS".to_owned(), shims.display().to_string()));
        // A login zsh puts other directories first again, so a zsh starts with the `.zshenv` of
        // neohar. That file gives the shell the `ZDOTDIR` of the user back.
        if is_zsh(&argv) {
            let user = match env.iter().position(|(key, _)| key == "ZDOTDIR") {
                Some(at) => Some(env.remove(at).1),
                None => factory
                    .base_env
                    .iter()
                    .find(|(key, _)| key == "ZDOTDIR")
                    .map(|(_, value)| value.clone()),
            };
            if let Some(user) = user {
                env.push(("NEOHAR_ZDOTDIR".to_owned(), user));
            }
            env.push((
                "ZDOTDIR".to_owned(),
                crate::shims::zdotdir(shims).display().to_string(),
            ));
        }
    }
    let socket = factory
        .paths
        .api_socket(&factory.session)
        .map(|path| path.display().to_string())
        .unwrap_or_default();
    // A test or development run never touches `~/.local/bin/nh`, so it names the binary itself.
    let bin = if factory.paths.has_overrides() {
        None
    } else {
        factory.paths.nh_symlink().ok()
    }
    .unwrap_or_else(|| factory.nh_path.clone());
    env.extend(
        [
            ("NEOHAR", "1".to_owned()),
            ("NEOHAR_SOCKET", socket),
            ("NEOHAR_PANE_ID", launch.pane.handle.clone()),
            ("NEOHAR_PANE_ULID", launch.pane.id.ulid().to_string()),
            ("NEOHAR_WORKSPACE_ID", launch.workspace_handle.clone()),
            ("NEOHAR_TAB_ID", launch.tab_handle.clone()),
            ("NEOHAR_SESSION", factory.session.clone()),
            ("NEOHAR_BIN", bin.display().to_string()),
            ("NEOHAR_PANE_TOKEN", token.to_owned()),
            ("TERM_PROGRAM_VERSION", env!("CARGO_PKG_VERSION").to_owned()),
        ]
        .map(|(key, value)| (key.to_owned(), value)),
    );
    PaneSpec {
        argv,
        cwd: spec.cwd.or_else(|| launch.root.clone()),
        env,
        cols: launch.pane.cols,
        rows: launch.pane.rows,
    }
}

/// Whether the program of a pane is zsh: `zsh`, `/bin/zsh`, or a login name like `-zsh`.
fn is_zsh(argv: &[String]) -> bool {
    argv.first()
        .and_then(|program| program.rsplit('/').next())
        .is_some_and(|name| name.trim_start_matches('-') == "zsh")
}
