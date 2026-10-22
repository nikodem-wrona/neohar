//! `askpass.ask`: the questions of ssh.
//!
//! The app starts ssh for a link with `SSH_ASKPASS` set to `nh askpass`. When ssh needs a host
//! key, a passphrase or a password, it runs `nh askpass "<question>"`, and that command calls this
//! method on the `api.sock` of the app. The core gives the question to the host in the process,
//! which shows a sheet and calls `done` with what the person typed.
//!
//! The answer is a secret. This module does not log it, does not put it in an event and does not
//! keep it after the reply. The method is for the CLI scope, so the token of a pane cannot call
//! it.

use std::sync::Arc;

use nh_proto::api::{
    calls::workspace::AskpassAnswered,
    methods::{Call, Method},
};
use serde_json::Value;

use crate::{
    ErrorCode,
    error::RpcError,
    host::{Host, SecretKind, SecretQuestion},
    runtime::Inner,
};

/// The longest question that the core takes. ssh questions are short; a host key question with a
/// long fingerprint is about 300 bytes.
const MAX_PROMPT: usize = 4096;

/// Whether this module runs the method.
pub(crate) fn handles(method: &str) -> bool {
    matches!(Method::from_name(method), Some(Method::AskpassAsk))
}

/// Runs a call that [`handles`] accepted.
pub(crate) async fn run(inner: &Arc<Inner>, call: Call) -> Result<Value, RpcError> {
    let Call::AskpassAsk(p) = call else {
        return Err(RpcError::internal(format!(
            "{} is not a method of askpass",
            call.method().name()
        )));
    };
    if p.prompt.len() > MAX_PROMPT {
        return Err(RpcError::invalid_params(format!(
            "the question is longer than {MAX_PROMPT} bytes"
        )));
    }
    let (answered, wait) = tokio::sync::oneshot::channel();
    inner.host.ask_secret(
        SecretQuestion {
            kind: SecretKind::of_prompt(&p.prompt),
            prompt: p.prompt,
            machine: p.machine,
        },
        Box::new(move |answer| {
            let _ = answered.send(answer);
        }),
    );
    // The sheet stays open while the person thinks. The call waits on its own task, not on the
    // state actor.
    let answer = wait.await.map_err(|_| {
        RpcError::new(
            ErrorCode::Unsupported,
            "This app cannot ask a question of ssh. Use an ssh key or the agent.",
        )
    })?;
    serde_json::to_value(AskpassAnswered { answer })
        .map_err(|error| RpcError::internal(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_host_key_question_is_known_by_its_words() {
        let key = "The authenticity of host 'studio (100.64.0.2)' can't be established.\n\
                   ED25519 key fingerprint is SHA256:abc.\n\
                   Are you sure you want to continue connecting (yes/no/[fingerprint])? ";
        assert_eq!(SecretKind::of_prompt(key), SecretKind::HostKey);
        assert_eq!(SecretKind::of_prompt("Password: "), SecretKind::Secret);
        assert_eq!(
            SecretKind::of_prompt("Enter passphrase for key '/Users/me/.ssh/id_ed25519': "),
            SecretKind::Secret
        );
        assert_eq!(
            SecretKind::of_prompt("Are You Sure You Want To Continue Connecting?"),
            SecretKind::HostKey
        );
    }
}
