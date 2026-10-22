//! `policy.list`, `.add`, `.remove` and `.suggest`.
//!
//! The rules of `config.toml` are listed, and `policy.remove` does not remove them: the file is
//! their only store. A rule that `policy.add` made is a row of `policy_rules` and a change of the
//! state in memory, in the same command.

use std::path::PathBuf;

use nh_proto::api::{
    calls::system::{
        PolicyAdded, PolicyListResult, PolicyRuleValue, PolicyScope, PolicySuggestResult,
    },
    events::{EventPayload, PolicyEvent},
    methods::{Call, Method},
    time::Timestamp,
};
use serde_json::{Value, to_value};

use super::{compile, compile_config, suggest, to_row};
use crate::{
    ErrorCode,
    actor::{Ctx, Handled, HandlerTable},
    error::RpcError,
};

fn internal(error: impl std::fmt::Display) -> RpcError {
    RpcError::internal(error.to_string())
}

/// The home of the user, for the `~` of a workspace.
pub(crate) fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

fn list(ctx: &Ctx<'_>, _: &mut nh_store::Tx<'_>, call: &Call) -> Result<Handled, RpcError> {
    if !matches!(call, Call::PolicyList(_)) {
        return Err(internal("not policy.list"));
    }
    let home = home();
    let (mut rules, _) = compile_config(&ctx.state.config.config(), home.as_deref());
    rules.extend(ctx.state.policy.table().iter().cloned());
    let rules = rules
        .iter()
        .map(|rule| PolicyRuleValue {
            id: rule.id.clone(),
            rule: rule.spec(),
            scope: rule.scope,
            created_at: rule.created_at,
        })
        .collect();
    Ok(Handled::result(
        to_value(PolicyListResult { rules }).map_err(internal)?,
    ))
}

fn add(ctx: &Ctx<'_>, tx: &mut nh_store::Tx<'_>, call: &Call) -> Result<Handled, RpcError> {
    let Call::PolicyAdd(p) = call else {
        return Err(internal("not policy.add"));
    };
    let home = home();
    let rule = compile(
        &p.rule,
        PolicyScope::Config,
        "api",
        Timestamp(ctx.now_ms),
        home.as_deref(),
    )
    .map_err(RpcError::invalid_params)?;
    let (file, _) = compile_config(&ctx.state.config.config(), home.as_deref());
    let taken = file.iter().any(|r| r.id == rule.id)
        || ctx.state.policy.table().iter().any(|r| r.id == rule.id);
    if taken {
        return Err(RpcError::new(
            ErrorCode::Conflict,
            format!("a policy rule with the id {} exists", rule.id),
        ));
    }
    tx.put_policy_rule(&to_row(&rule)).map_err(internal)?;
    tx.append(
        Some(&rule.id),
        &EventPayload::PolicyAdded(PolicyEvent {
            id: rule.id.clone(),
            scope: rule.scope,
        }),
    )
    .map_err(internal)?;
    let id = rule.id.clone();
    Ok(Handled {
        result: to_value(PolicyAdded { id }).map_err(internal)?,
        mutation: Some(Box::new(move |state, _| state.policy.put(rule))),
    })
}

fn remove(ctx: &Ctx<'_>, tx: &mut nh_store::Tx<'_>, call: &Call) -> Result<Handled, RpcError> {
    let Call::PolicyRemove(p) = call else {
        return Err(internal("not policy.remove"));
    };
    let Some(rule) = ctx
        .state
        .policy
        .table()
        .iter()
        .find(|r| r.id == p.id)
        .cloned()
    else {
        let home = home();
        let (file, _) = compile_config(&ctx.state.config.config(), home.as_deref());
        return Err(if file.iter().any(|r| r.id == p.id) {
            RpcError::new(
                ErrorCode::ConfigRule,
                format!("the rule {} is in config.toml; remove it there", p.id),
            )
        } else {
            RpcError::new(ErrorCode::NotFound, format!("no policy rule {}", p.id))
        });
    };
    tx.delete_policy_rule(&rule.id).map_err(internal)?;
    tx.append(
        Some(&rule.id),
        &EventPayload::PolicyRemoved(PolicyEvent {
            id: rule.id.clone(),
            scope: rule.scope,
        }),
    )
    .map_err(internal)?;
    let id = rule.id;
    Ok(Handled {
        result: Value::Object(serde_json::Map::new()),
        mutation: Some(Box::new(move |state, _| {
            state.policy.remove(&id);
        })),
    })
}

fn suggestions(ctx: &Ctx<'_>, tx: &mut nh_store::Tx<'_>, call: &Call) -> Result<Handled, RpcError> {
    if !matches!(call, Call::PolicySuggest(_)) {
        return Err(internal("not policy.suggest"));
    }
    let rows = tx
        .fingerprints_since(ctx.now_ms - suggest::WINDOW_MS)
        .map_err(internal)?;
    let suggestions = suggest::suggest(&rows, ctx.now_ms);
    Ok(Handled::result(
        to_value(PolicySuggestResult { suggestions }).map_err(internal)?,
    ))
}

/// The handlers of this module.
pub(crate) fn handlers(table: &mut HandlerTable) {
    table.insert(Method::PolicyList, list);
    table.insert(Method::PolicyAdd, add);
    table.insert(Method::PolicyRemove, remove);
    table.insert(Method::PolicySuggest, suggestions);
}
