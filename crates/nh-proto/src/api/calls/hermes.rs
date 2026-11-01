//! Params and results of `hermes.*`: the chats with Hermes Agent.
//!
//! The core runs `hermes acp` for each open chat and speaks the Agent Client Protocol (ACP) to
//! it. The gateway of Hermes owns the session, so a turn continues when the app quits. The core
//! resumes the session when it starts again. `hermes_chats` has the chats, and
//! `hermes_chat:<chat>` the items of one chat.

use crate::api::{common::wire, ids::ChatId};

wire! {
    /// `hermes.check`: whether this machine can run Hermes.
    pub struct HermesStatus {
        /// `hermes acp --version` answered.
        pub found: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub version: Option<String>,
        /// `hermes acp --check` passed.
        pub ok: bool,
        /// `approvals.mode` of the config of Hermes. With `smart` or `off`, shell commands do not
        /// ask in the chat.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub approvals_mode: Option<String>,
        /// What to do, when Hermes is not found or the check failed.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub message: Option<String>,
    }

    pub struct HermesStart {
        /// The absolute path of the folder in which Hermes works.
        pub cwd: String,
        /// The first prompt.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub prompt: Option<String>,
    }

    pub struct HermesStarted {
        pub chat: ChatId,
    }

    pub struct HermesChatRef {
        pub chat: ChatId,
    }

    pub struct HermesPrompt {
        pub chat: ChatId,
        pub text: String,
        /// A second call with the same id sends nothing.
        pub input_id: u64,
    }

    pub struct HermesAnswer {
        pub chat: ChatId,
        /// The `request` of the approval item.
        pub request: String,
        /// The `id` of one of its options.
        pub option: String,
    }

    pub struct HermesSet {
        pub chat: ChatId,
        /// The id of a model of the chat.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub model: Option<String>,
        /// The id of a mode of the chat.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub mode: Option<String>,
    }
}
