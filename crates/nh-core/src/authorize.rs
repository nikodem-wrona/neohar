//! Who may call which method.
//!
//! Every method of the contract lists the callers that may use it ([`Scopes`]). A caller has one
//! scope, which the entry point gives:
//!
//! - **Ui**: the host, for the webview (`CoreHandle::rpc`).
//! - **Cli**: a client of the API socket with no pane token. A client of this scope that runs
//!   inside a pane of the core may not answer interactions or weaken the approvals: the API socket
//!   refuses these calls before they get here ([`crate::in_pane`]).
//! - **Pane**: a request with the token of a pane. Pane scope can call only the methods for panes
//!   (`adapter.*`), so a request with the token cannot answer an interaction. This does not make a
//!   process in a pane harmless: it runs as the user, it can connect without the token, and then it
//!   is a client of the CLI. [`crate::in_pane`] stops that plain path, and a process that leaves
//!   the process tree of its pane gets past it. The approval gate is not a sandbox.
//! - **Phone**: a paired phone over the listener. It has the methods of the UI,
//!   less the refusals of [`Method::phone_refusal`], and the methods that only a phone calls.
//! - **Dev**: the methods of development and test builds (`debug.*`) exist only in a build with the
//!   feature `debug`. In such a build any caller except a pane or a phone may call them. Some debug
//!   methods are not in the contract (`crate::debug_calls`). The same rule applies to them.
//!
//! A method that does not exist is `method_not_found`. A method that exists and is not for this
//! caller is `permission_denied`.

use nh_proto::api::methods::{Method, PhoneRefusal};

use crate::{
    ErrorCode,
    actor::{Origin, Scope},
    error::RpcError,
};

/// The method that `name` is, or `None` for a development method that is not in the contract.
/// Fails when `origin` may not call it.
pub(crate) fn authorize(name: &str, origin: &Origin) -> Result<Option<Method>, RpcError> {
    let debug_build = cfg!(feature = "debug");
    let Some(method) = Method::from_name(name) else {
        // Debug methods that are not in the contract are outside the table.
        return if debug_build
            && name.starts_with("debug.")
            && !matches!(origin.scope, Scope::Pane | Scope::Phone)
        {
            Ok(None)
        } else {
            Err(RpcError::method_not_found(name))
        };
    };
    let scopes = method.scopes();
    if scopes.dev {
        if !debug_build {
            return Err(RpcError::method_not_found(name));
        }
        return if matches!(origin.scope, Scope::Pane | Scope::Phone) {
            Err(denied(name, origin.scope))
        } else {
            Ok(Some(method))
        };
    }
    if origin.scope == Scope::Phone
        && let Some(refusal) = method.phone_refusal()
    {
        return Err(match refusal {
            PhoneRefusal::Unsupported(why) => RpcError::new(ErrorCode::Unsupported, why),
            PhoneRefusal::Denied(why) => RpcError::new(ErrorCode::PermissionDenied, why),
        });
    }
    let allowed = match origin.scope {
        Scope::Ui => scopes.ui,
        Scope::Cli => scopes.cli,
        Scope::Pane => scopes.pane,
        // A caller of the dev scope has the powers of the CLI.
        Scope::Dev => scopes.cli,
        Scope::Phone => method.phone_may_call(),
    };
    if allowed {
        Ok(Some(method))
    } else {
        Err(denied(name, origin.scope))
    }
}

fn denied(name: &str, scope: Scope) -> RpcError {
    let who = match scope {
        Scope::Ui => "the UI",
        Scope::Cli | Scope::Dev => "this client",
        Scope::Pane => "a pane",
        Scope::Phone => "a phone",
    };
    RpcError::new(
        ErrorCode::PermissionDenied,
        format!("{who} may not call {name}"),
    )
}

#[cfg(test)]
mod tests {
    use ulid::Ulid;

    use super::*;

    fn ui() -> Origin {
        Origin::ui()
    }
    fn cli() -> Origin {
        Origin::cli(7)
    }
    fn pane() -> Origin {
        Origin::pane(7, Ulid::from(1u128))
    }

    #[test]
    fn a_method_is_for_the_scopes_that_the_table_lists() {
        // `pane.input` is for the UI, `pane.read` for the CLI, `session.info` for both.
        assert!(authorize("pane.input", &ui()).is_ok());
        assert_eq!(
            authorize("pane.input", &cli()).expect_err("UI only").code,
            ErrorCode::PermissionDenied
        );
        assert!(authorize("pane.read", &cli()).is_ok());
        assert_eq!(
            authorize("pane.read", &ui()).expect_err("CLI only").code,
            ErrorCode::PermissionDenied
        );
        assert!(authorize("session.info", &ui()).is_ok());
        assert!(authorize("session.info", &cli()).is_ok());
    }

    #[test]
    fn a_pane_can_call_the_adapter_methods_and_nothing_else() {
        for method in ["adapter.signal", "adapter.gate", "adapter.gate_resume"] {
            assert!(authorize(method, &pane()).is_ok(), "{method}");
            assert_eq!(
                authorize(method, &cli()).expect_err("needs a token").code,
                ErrorCode::PermissionDenied,
                "{method}"
            );
            assert_eq!(
                authorize(method, &ui()).expect_err("not for the UI").code,
                ErrorCode::PermissionDenied,
                "{method}"
            );
        }
        for method in [
            "interaction.answer",
            "session.info",
            "config.set",
            "policy.add",
        ] {
            let error = authorize(method, &pane()).expect_err(method);
            assert_eq!(error.code, ErrorCode::PermissionDenied, "{method}");
        }
    }

    #[test]
    fn a_phone_has_the_methods_of_the_ui_less_the_refusals() {
        let phone = Origin::phone(Ulid::from(9u128));
        for method in [
            "pane.input",
            "config.set",
            "interaction.answer",
            "agent.reply",
            "pane.claim",
            "window.set_single",
            "phone.window",
            "phone.visible",
            "pane.read",
        ] {
            assert!(authorize(method, &phone).is_ok(), "{method}");
        }
        for method in ["window.open", "window.close", "pane.popout", "fs.open"] {
            assert_eq!(
                authorize(method, &phone).expect_err(method).code,
                ErrorCode::Unsupported,
                "{method}"
            );
        }
        for method in ["phone.pair_begin", "phone.publish", "session.shutdown"] {
            assert_eq!(
                authorize(method, &phone).expect_err(method).code,
                ErrorCode::PermissionDenied,
                "{method}"
            );
        }
        // The UI cannot call what only a phone calls.
        assert_eq!(
            authorize("phone.window", &ui())
                .expect_err("a phone only")
                .code,
            ErrorCode::PermissionDenied
        );
    }

    #[test]
    fn an_unknown_method_is_not_found_for_every_caller() {
        for origin in [ui(), cli(), pane()] {
            assert_eq!(
                authorize("nothing.here", &origin)
                    .expect_err("unknown")
                    .code,
                ErrorCode::MethodNotFound
            );
        }
    }

    #[cfg(feature = "debug")]
    #[test]
    fn debug_methods_exist_in_a_debug_build_and_not_for_a_pane() {
        assert!(authorize("debug.screen", &ui()).is_ok());
        assert!(authorize("debug.screen", &cli()).is_ok());
        assert_eq!(
            authorize("debug.screen", &pane()).expect_err("a pane").code,
            ErrorCode::PermissionDenied
        );
        // A debug method that the table does not have.
        assert_eq!(authorize("debug.clock", &ui()).expect("allowed"), None);
        assert_eq!(
            authorize("debug.clock", &pane()).expect_err("a pane").code,
            ErrorCode::MethodNotFound
        );
    }

    #[cfg(not(feature = "debug"))]
    #[test]
    fn debug_methods_do_not_exist_without_the_feature() {
        assert_eq!(
            authorize("debug.screen", &ui())
                .expect_err("no such method")
                .code,
            ErrorCode::MethodNotFound
        );
        assert_eq!(
            authorize("debug.clock", &cli())
                .expect_err("no such method")
                .code,
            ErrorCode::MethodNotFound
        );
    }
}
