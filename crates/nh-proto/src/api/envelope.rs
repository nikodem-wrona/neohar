//! JSON-RPC 2.0 envelopes and the error shape of the API.
//!
//! The API server speaks newline-delimited JSON-RPC 2.0. A protocol error uses the reserved
//! integer of JSON-RPC. An error of the application uses `-32000` and puts a string code and the
//! details in `error.data`. A client shows `{code, message, details}` ([`CallError`]) and acts on
//! the string.

use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use serde_json::Value;

/// The `jsonrpc` member. It is always `"2.0"`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct JsonRpc;

impl Serialize for JsonRpc {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str("2.0")
    }
}

impl<'de> Deserialize<'de> for JsonRpc {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        if text == "2.0" {
            Ok(Self)
        } else {
            Err(de::Error::custom(format!(
                "jsonrpc must be \"2.0\", not \"{text}\""
            )))
        }
    }
}

impl JsonSchema for JsonRpc {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "JsonRpc".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({"const": "2.0", "type": "string"})
    }
}

/// The id of a request: a number or a string.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum RequestId {
    Number(i64),
    Text(String),
}

/// A request. A request without an `id` is a notification of the client, and gets no response.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Request {
    pub jsonrpc: JsonRpc,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<RequestId>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

/// A response. Exactly one of `result` and `error` is set. `id` is `null` when the request was too
/// broken to have one.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Response {
    pub jsonrpc: JsonRpc,
    pub id: Option<RequestId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

impl Response {
    pub fn ok(id: Option<RequestId>, result: Value) -> Self {
        Self {
            jsonrpc: JsonRpc,
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn err(id: Option<RequestId>, error: RpcError) -> Self {
        Self {
            jsonrpc: JsonRpc,
            id,
            result: None,
            error: Some(error),
        }
    }
}

/// A message of the server without an id: `events.event`, `projection.message` and the like.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Notification {
    pub jsonrpc: JsonRpc,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

impl Notification {
    pub fn new(method: impl Into<String>, params: Value) -> Self {
        Self {
            jsonrpc: JsonRpc,
            method: method.into(),
            params,
        }
    }
}

/// The string code of an error. The codes are part of the contract, and this list is complete: the
/// core, the hosts and the link send no other code. To send a new code, add it here first. The UI
/// and the CLI act on a few of them (`already_answered`, `confirm_required`, `not_found`,
/// `unsupported`). On the wire a code is its `snake_case` name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// The line is not JSON (-32700).
    ParseError,
    /// The JSON is not a request (-32600).
    InvalidRequest,
    /// The method does not exist (-32601).
    MethodNotFound,
    /// The params do not fit the method (-32602).
    InvalidParams,
    /// The thing that the call names does not exist.
    NotFound,
    /// The pane exists and has no process: its holder starts, the program ended before the core
    /// restarted, or the holder is lost. `pane.restart` starts the program again.
    PaneGone,
    /// The caller may not call this method, or not on this thing.
    PermissionDenied,
    /// Another client answered the interaction first. `details` has `{by, via, decision}`.
    AlreadyAnswered,
    /// An input was in flight when the holder or the core went away. `details` has `{input_id, reason}`.
    InputUnconfirmed,
    /// The core cannot say whether an answer reached the harness.
    DeliveryUnknown,
    /// The store is read-only or full. The core runs, but it does not keep changes.
    DegradedStorage,
    /// The call needs a second call with `force`. `details` says what the user would lose.
    ConfirmRequired,
    /// The caller worked with an older version of the thing. Read it again.
    StaleVersion,
    /// The core is busy with the same work. Try again.
    Busy,
    /// The harness of a start is not installed, or it is logged out after a new probe.
    /// `details` has `{harness, reason, hint}`, and `reason` is `not_installed` or
    /// `logged_out`.
    HarnessUnavailable,
    /// The method exists in the contract, and the core has no handler for it yet.
    Unsupported,
    /// The link to the core of a machine is down. A call fails with it at once, and a
    /// call in flight when the link went down fails with it too: its outcome is unknown.
    LinkDown,
    /// The core of a machine has another major version of the API.
    VersionMismatch,
    /// `agent.reply` on a run that works or has an open interaction. `details` has
    /// `{execution, pending}`.
    NotIdle,
    /// The pane runs a program or an agent, and the call needs an idle shell.
    PaneBusy,
    /// The core cannot start the holder or the program of a pane.
    PaneSpawnFailed,
    /// The holder of a pane failed or sent something that the core does not accept.
    HolderError,
    /// The agent still runs, and the call needs an agent that has ended.
    AgentLive,
    /// The agent has ended, and the call needs a live agent.
    AgentGone,
    /// The run is not linked to its pane, so the core types nothing into it.
    NotBound,
    /// A live agent has the name already.
    NameTaken,
    /// The run in the pane is of another harness than the call names.
    HarnessMismatch,
    /// The manifest of a start has a value that is not correct.
    ManifestError,
    /// The interaction has ended. `details` has `{state}`.
    InteractionClosed,
    /// No channel reaches the harness of the interaction, so nothing can answer it.
    NotAnswerable,
    /// This core does not hold the gates of the hooks.
    GateUnavailable,
    /// The job of the call was cancelled.
    Cancelled,
    /// The work of the call did not end in time.
    Timeout,
    /// The request, the file or the blob is larger than the limit.
    TooLarge,
    /// The bytes of a blob do not agree with its hash.
    BlobCorrupt,
    /// A policy rule with the same id exists.
    Conflict,
    /// The policy rule is in `config.toml`, so only a change of that file removes it.
    ConfigRule,
    /// A command of git failed.
    GitFailed,
    /// The core cannot forward the port of a machine.
    ForwardFailed,
    /// The client decodes another version of the frame format.
    ProtoUnsupported,
    /// The core cannot write a file.
    IoError,
    /// The database failed.
    StoreError,
    /// A fault of the core. The log has the reason.
    Internal,
}

impl ErrorCode {
    /// The integer of JSON-RPC.
    pub const fn rpc_code(self) -> i32 {
        match self {
            Self::ParseError => -32700,
            Self::InvalidRequest => -32600,
            Self::MethodNotFound => -32601,
            Self::InvalidParams => -32602,
            Self::Internal => -32603,
            _ => -32000,
        }
    }

    /// The string code for a reserved integer of JSON-RPC.
    pub const fn from_reserved(code: i32) -> Option<Self> {
        match code {
            -32700 => Some(Self::ParseError),
            -32600 => Some(Self::InvalidRequest),
            -32601 => Some(Self::MethodNotFound),
            -32602 => Some(Self::InvalidParams),
            -32603 => Some(Self::Internal),
            _ => None,
        }
    }

    /// Every code, in the order of the enum.
    pub const ALL: &'static [Self] = &[
        Self::ParseError,
        Self::InvalidRequest,
        Self::MethodNotFound,
        Self::InvalidParams,
        Self::NotFound,
        Self::PaneGone,
        Self::PermissionDenied,
        Self::AlreadyAnswered,
        Self::InputUnconfirmed,
        Self::DeliveryUnknown,
        Self::DegradedStorage,
        Self::ConfirmRequired,
        Self::StaleVersion,
        Self::Busy,
        Self::HarnessUnavailable,
        Self::Unsupported,
        Self::LinkDown,
        Self::VersionMismatch,
        Self::NotIdle,
        Self::PaneBusy,
        Self::PaneSpawnFailed,
        Self::HolderError,
        Self::AgentLive,
        Self::AgentGone,
        Self::NotBound,
        Self::NameTaken,
        Self::HarnessMismatch,
        Self::ManifestError,
        Self::InteractionClosed,
        Self::NotAnswerable,
        Self::GateUnavailable,
        Self::Cancelled,
        Self::Timeout,
        Self::TooLarge,
        Self::BlobCorrupt,
        Self::Conflict,
        Self::ConfigRule,
        Self::GitFailed,
        Self::ForwardFailed,
        Self::ProtoUnsupported,
        Self::IoError,
        Self::StoreError,
        Self::Internal,
    ];

    /// The string code, as it is on the wire.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ParseError => "parse_error",
            Self::InvalidRequest => "invalid_request",
            Self::MethodNotFound => "method_not_found",
            Self::InvalidParams => "invalid_params",
            Self::NotFound => "not_found",
            Self::PaneGone => "pane_gone",
            Self::PermissionDenied => "permission_denied",
            Self::AlreadyAnswered => "already_answered",
            Self::InputUnconfirmed => "input_unconfirmed",
            Self::DeliveryUnknown => "delivery_unknown",
            Self::DegradedStorage => "degraded_storage",
            Self::ConfirmRequired => "confirm_required",
            Self::StaleVersion => "stale_version",
            Self::Busy => "busy",
            Self::HarnessUnavailable => "harness_unavailable",
            Self::Unsupported => "unsupported",
            Self::LinkDown => "link_down",
            Self::VersionMismatch => "version_mismatch",
            Self::NotIdle => "not_idle",
            Self::PaneBusy => "pane_busy",
            Self::PaneSpawnFailed => "pane_spawn_failed",
            Self::HolderError => "holder_error",
            Self::AgentLive => "agent_live",
            Self::AgentGone => "agent_gone",
            Self::NotBound => "not_bound",
            Self::NameTaken => "name_taken",
            Self::HarnessMismatch => "harness_mismatch",
            Self::ManifestError => "manifest_error",
            Self::InteractionClosed => "interaction_closed",
            Self::NotAnswerable => "not_answerable",
            Self::GateUnavailable => "gate_unavailable",
            Self::Cancelled => "cancelled",
            Self::Timeout => "timeout",
            Self::TooLarge => "too_large",
            Self::BlobCorrupt => "blob_corrupt",
            Self::Conflict => "conflict",
            Self::ConfigRule => "config_rule",
            Self::GitFailed => "git_failed",
            Self::ForwardFailed => "forward_failed",
            Self::ProtoUnsupported => "proto_unsupported",
            Self::IoError => "io_error",
            Self::StoreError => "store_error",
            Self::Internal => "internal",
        }
    }

    /// The code for its string on the wire. `None` for a code that this version does not know: a
    /// core of another version can send one.
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|code| code.as_str() == text)
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The `error.data` of an application error.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ErrorData {
    pub code: ErrorCode,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub details: Value,
}

/// The `error` member of a response.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RpcError {
    /// The integer of JSON-RPC: a reserved one for a protocol error, else `-32000`.
    pub code: i32,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<ErrorData>,
}

impl RpcError {
    /// An error with a code and the details. The integer follows from the code.
    pub fn new(code: ErrorCode, message: impl Into<String>, details: Value) -> Self {
        Self {
            code: code.rpc_code(),
            message: message.into(),
            data: Some(ErrorData { code, details }),
        }
    }

    /// An error without details.
    pub fn plain(code: ErrorCode, message: impl Into<String>) -> Self {
        Self::new(code, message, Value::Null)
    }

    /// What a client shows: the string code, the message and the details.
    pub fn call_error(&self) -> CallError {
        match &self.data {
            Some(data) => CallError {
                code: data.code,
                message: self.message.clone(),
                details: data.details.clone(),
            },
            None => CallError {
                code: ErrorCode::from_reserved(self.code).unwrap_or(ErrorCode::Internal),
                message: self.message.clone(),
                details: Value::Null,
            },
        }
    }
}

impl fmt::Display for RpcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.call_error().code, self.message)
    }
}

impl std::error::Error for RpcError {}

/// The error as a caller sees it: `{code, message, details}`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CallError {
    pub code: ErrorCode,
    pub message: String,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub details: Value,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_request_without_params_or_id_decodes() {
        let request: Request =
            serde_json::from_str(r#"{"jsonrpc":"2.0","method":"session.info"}"#).expect("decodes");
        assert_eq!(request.id, None);
        assert_eq!(request.params, Value::Null);
    }

    #[test]
    fn a_wrong_version_is_refused() {
        assert!(serde_json::from_str::<Request>(r#"{"jsonrpc":"1.0","method":"x"}"#).is_err());
    }

    #[test]
    fn the_id_is_a_number_or_a_string() {
        let number: RequestId = serde_json::from_str("7").expect("number");
        let text: RequestId = serde_json::from_str(r#""a-1""#).expect("text");
        assert_eq!(number, RequestId::Number(7));
        assert_eq!(text, RequestId::Text("a-1".to_owned()));
    }

    #[test]
    fn an_application_error_is_32000_with_a_string_code_in_data() {
        let error = RpcError::new(
            ErrorCode::AlreadyAnswered,
            "answered by w3",
            json!({"by": "w3"}),
        );
        let wire = serde_json::to_value(&error).expect("serializes");
        assert_eq!(
            wire,
            json!({"code": -32000, "message": "answered by w3",
                   "data": {"code": "already_answered", "details": {"by": "w3"}}})
        );
        let call = error.call_error();
        assert_eq!(call.code, ErrorCode::AlreadyAnswered);
        assert_eq!(call.details, json!({"by": "w3"}));
    }

    #[test]
    fn a_protocol_error_keeps_the_reserved_integer() {
        let error = RpcError::plain(ErrorCode::MethodNotFound, "no such method");
        assert_eq!(error.code, -32601);
        let bare = RpcError {
            code: -32602,
            message: "bad".to_owned(),
            data: None,
        };
        assert_eq!(bare.call_error().code, ErrorCode::InvalidParams);
    }

    #[test]
    fn the_reserved_integers_and_the_codes_agree() {
        for code in ErrorCode::ALL {
            if let Some(back) = ErrorCode::from_reserved(code.rpc_code()) {
                assert_eq!(&back, code);
            } else {
                assert_eq!(code.rpc_code(), -32000);
            }
            let text = serde_json::to_value(code).expect("serializes");
            assert_eq!(text, Value::String(code.as_str().to_owned()));
        }
    }

    #[test]
    fn every_code_is_in_all_and_parses_back() {
        // The schema lists every variant that serde knows, so a code missing from `ALL` fails here.
        let schema = serde_json::to_value(schemars::schema_for!(ErrorCode)).expect("schema");
        let listed: Vec<&str> = schema["oneOf"]
            .as_array()
            .expect("one schema for each code")
            .iter()
            .filter_map(|variant| variant["const"].as_str())
            .collect();
        let all: Vec<&str> = ErrorCode::ALL.iter().map(|code| code.as_str()).collect();
        assert_eq!(listed, all);
        for code in ErrorCode::ALL {
            assert_eq!(ErrorCode::parse(code.as_str()), Some(*code));
        }
        assert_eq!(ErrorCode::parse("unknown_pane"), None);
    }

    #[test]
    fn a_response_has_a_result_or_an_error() {
        let ok = Response::ok(Some(RequestId::Number(1)), json!({}));
        let text = serde_json::to_string(&ok).expect("serializes");
        assert_eq!(text, r#"{"jsonrpc":"2.0","id":1,"result":{}}"#);
        let err = Response::err(None, RpcError::plain(ErrorCode::ParseError, "bad json"));
        let text = serde_json::to_string(&err).expect("serializes");
        assert!(text.contains(r#""id":null"#) && text.contains(r#""code":-32700"#));
    }
}
