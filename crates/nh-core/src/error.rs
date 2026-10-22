//! Errors of the core.

use std::path::PathBuf;

pub use nh_proto::api::envelope::ErrorCode;
use serde::Serialize;
use serde_json::{Value, json};

/// An error of an API method.
///
/// It has the shape that clients get: a `code` that code can match on, a `message` for people,
/// and `details`. The hosts pass it to the webview as it is.
#[derive(Clone, Debug, PartialEq, Serialize, thiserror::Error)]
#[error("{code}: {message}")]
pub struct RpcError {
    /// A code of the contract. On the wire it is its `snake_case` name, for example `not_found`.
    pub code: ErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Value::is_null")]
    pub details: Value,
}

impl RpcError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: Value::Null,
        }
    }

    /// An error with the code that a peer sent as a string: the core of another machine, or of
    /// another version. An older core can send a code that is now merged into another one. A code
    /// that this version does not know becomes `internal`, and `details` keeps the code and the
    /// details that the peer sent.
    pub fn from_wire(code: &str, message: impl Into<String>, details: Value) -> Self {
        let merged = match code {
            "unknown_pane" | "unknown_interaction" | "unknown_projection" | "unknown_stream" => {
                Some(ErrorCode::NotFound)
            }
            "request_too_large" | "blob_too_large" => Some(ErrorCode::TooLarge),
            _ => ErrorCode::parse(code),
        };
        let mut error = Self::new(merged.unwrap_or(ErrorCode::Internal), message);
        error.details = match merged {
            Some(_) => details,
            None => json!({ "code": code, "details": details }),
        };
        error
    }

    /// The parameters of a method do not have the expected shape.
    pub fn invalid_params(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidParams, message)
    }

    pub fn method_not_found(method: &str) -> Self {
        Self::new(
            ErrorCode::MethodNotFound,
            format!("unknown method {method:?}"),
        )
    }

    pub fn unknown_pane(pane: impl std::fmt::Display) -> Self {
        Self::new(ErrorCode::NotFound, format!("no pane {pane}"))
    }

    /// A failure of the core itself, not of the request.
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Internal, message)
    }
}

/// An error from starting the core or a pane.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error(transparent)]
    Paths(#[from] nh_proto::paths::PathsError),
    #[error("cannot start the runtime: {0}")]
    Runtime(std::io::Error),
    #[error("cannot prepare {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("another core serves {0}")]
    ApiSocketInUse(PathBuf),
    #[error("no random bytes: {0}")]
    Random(getrandom::Error),
    #[error(transparent)]
    Spawn(#[from] crate::holder::spawn::SpawnError),
    #[error(transparent)]
    Holder(#[from] crate::holder::HolderError),
    #[error("the database failed: {0}")]
    Store(#[from] nh_store::StoreError),
    /// The layout dropped the pane while its holder started.
    #[error("the pane {0} is gone")]
    PaneGone(ulid::Ulid),
}

impl From<CoreError> for RpcError {
    fn from(error: CoreError) -> Self {
        let code = match &error {
            CoreError::Spawn(_) => ErrorCode::PaneSpawnFailed,
            CoreError::Holder(_) => ErrorCode::HolderError,
            CoreError::Store(_) => ErrorCode::StoreError,
            CoreError::PaneGone(_) => ErrorCode::PaneGone,
            _ => ErrorCode::Internal,
        };
        Self::new(code, error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_code_is_its_string_on_the_wire() {
        let error = RpcError::new(ErrorCode::PaneBusy, "busy");
        let wire = serde_json::to_value(&error).expect("serializes");
        assert_eq!(wire, json!({"code": "pane_busy", "message": "busy"}));
    }

    #[test]
    fn a_code_from_the_wire_that_is_not_known_becomes_internal() {
        let known = RpcError::from_wire("not_found", "gone", json!({"pane": 1}));
        assert_eq!(known.code, ErrorCode::NotFound);
        assert_eq!(known.details, json!({"pane": 1}));
        let unknown = RpcError::from_wire("new_code", "later", json!({"a": 1}));
        assert_eq!(unknown.code, ErrorCode::Internal);
        assert_eq!(
            unknown.details,
            json!({"code": "new_code", "details": {"a": 1}})
        );
        let internal = RpcError::from_wire("internal", "fault", Value::Null);
        assert_eq!(internal.details, Value::Null);
        let old = RpcError::from_wire("blob_too_large", "big", Value::Null);
        assert_eq!(old.code, ErrorCode::TooLarge);
    }
}
