//! Transcripts: what the harnesses write about a session.
//!
//! The harness writes one JSON object on each line of a file. This module turns the lines into
//! [`Entry`] values, counts tokens without counting one message twice, prices them, and builds the
//! items of the timeline. It does no I/O. The core reads the files and gives the lines
//! to the functions here.
//!
//! The parsers are lenient. A line that is not JSON, or has a type that we do not know, gives no
//! entry. A field that is missing gives `None`. The formats change with the versions of the
//! harnesses, and a new line type must never stop the tailer.

use nh_proto::api::time::Timestamp;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub mod buffer;
mod claude;
mod codex;
pub mod ledger;
pub mod prices;
pub mod timeline;

pub use buffer::{Line, LineBuffer};
pub use ledger::{Billing, Ledger, Saved, UsageDelta};
pub use prices::Prices;
pub use timeline::Builder;

/// The longest preview of a prompt or a reply, in characters.
pub const PREVIEW_CHARS: usize = 300;
/// The longest output of a tool that the timeline keeps, in characters.
pub const OUTPUT_CHARS: usize = 1000;

/// The format of a transcript file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Format {
    /// `~/.claude/projects/<cwd-slug>/<session>.jsonl`.
    Claude,
    /// A rollout under `~/.codex/sessions/`.
    Codex,
}

/// Tokens of one call to the model. `input` does not hold the cached tokens: Codex counts them in
/// its input, and the parser takes them out. The sum of the four figures is what the call read and
/// wrote.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    /// The part of `cache_write` that went to the cache of one hour. It costs more.
    pub cache_write_1h: u64,
}

impl TokenUsage {
    pub fn is_zero(&self) -> bool {
        self.input == 0 && self.output == 0 && self.cache_read == 0 && self.cache_write == 0
    }

    /// The tokens that the call sent to the model. The price can depend on this figure.
    pub fn prompt(&self) -> u64 {
        self.input + self.cache_read + self.cache_write
    }

    pub fn add(&mut self, other: &TokenUsage) {
        self.input += other.input;
        self.output += other.output;
        self.cache_read += other.cache_read;
        self.cache_write += other.cache_write;
        self.cache_write_1h += other.cache_write_1h;
    }

    /// What `self` has more than `seen`, field by field. A field that got smaller counts for zero.
    pub fn since(&self, seen: &TokenUsage) -> TokenUsage {
        TokenUsage {
            input: self.input.saturating_sub(seen.input),
            output: self.output.saturating_sub(seen.output),
            cache_read: self.cache_read.saturating_sub(seen.cache_read),
            cache_write: self.cache_write.saturating_sub(seen.cache_write),
            cache_write_1h: self.cache_write_1h.saturating_sub(seen.cache_write_1h),
        }
    }

    /// The larger of each field.
    pub fn max(&self, other: &TokenUsage) -> TokenUsage {
        TokenUsage {
            input: self.input.max(other.input),
            output: self.output.max(other.output),
            cache_read: self.cache_read.max(other.cache_read),
            cache_write: self.cache_write.max(other.cache_write),
            cache_write_1h: self.cache_write_1h.max(other.cache_write_1h),
        }
    }
}

/// A part of a message of the model.
#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    Text(String),
    ToolUse {
        /// The id of the call. The result names it.
        id: String,
        name: String,
        /// One line about the input: the command, the path or the pattern.
        summary: String,
        /// For a call that starts a subagent: its type.
        subagent: Option<String>,
    },
}

/// A window of a rate limit as a harness reports it in its transcript.
#[derive(Clone, Debug, PartialEq)]
pub struct LimitWindow {
    pub name: String,
    /// From 0 to 100.
    pub used_percent: f32,
    pub resets_at: Option<Timestamp>,
}

/// What a line says.
#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    /// A prompt of the person. It starts a turn.
    Prompt { text: String },
    /// A message of the model. `message` is its id in the harness, which the lines of one message
    /// share.
    Assistant {
        message: Option<String>,
        model: Option<String>,
        usage: Option<TokenUsage>,
        blocks: Vec<Block>,
    },
    /// The result of a call to a tool.
    ToolResult {
        tool_use: String,
        failed: bool,
        /// The harness refused the call: the person, or a rule, said no.
        denied: bool,
        text: String,
    },
    /// A turn starts (Codex). Claude has no such line.
    TurnStart,
    /// A turn ends.
    TurnEnd { duration_ms: Option<u64> },
    /// The model of the next calls (Codex writes it with each turn).
    Model { model: String },
    /// Tokens of one response, when the harness reports them apart from the message (Codex).
    Usage {
        key: String,
        model: Option<String>,
        usage: TokenUsage,
    },
    /// The running cost of the session, as the harness counts it (Claude). The figures are totals,
    /// not the cost of one turn.
    Cost { models: Vec<ModelCost> },
    /// Rate limit windows (Codex).
    Limits { windows: Vec<LimitWindow> },
    /// The harness compacted the conversation.
    Compaction,
}

/// The total cost of one model in a session.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelCost {
    pub model: String,
    pub usd: f64,
}

/// One thing that a line says. A line can say several things: a message with two tool results.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    /// A stable id of the line, from the harness when it has one, else from the offset. The items
    /// of the timeline take their ids from it.
    pub id: String,
    pub at: Option<Timestamp>,
    /// The line belongs to a subagent. It counts for the usage and not for the timeline.
    pub sidechain: bool,
    pub kind: Kind,
}

/// The entries of one line. `offset` is the position of the line in the file; it makes the id of a
/// line that has none.
pub fn parse_line(format: Format, line: &str, offset: u64) -> Vec<Entry> {
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        return Vec::new();
    };
    match format {
        Format::Claude => claude::parse(&value, offset),
        Format::Codex => codex::parse(&value, offset),
    }
}

// --- helpers for the parsers -----------------------------------------------------------------

pub(crate) fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

pub(crate) fn number(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or(0)
}

pub(crate) fn moment(value: &Value) -> Option<Timestamp> {
    text(value, "timestamp").and_then(|t| Timestamp::parse(t).ok())
}

/// The first `max` characters of a text, on one line, with an ellipsis when it was cut.
pub fn preview(text: &str, max: usize) -> String {
    let flat: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max {
        return flat;
    }
    let mut cut: String = flat.chars().take(max).collect();
    cut.push('…');
    cut
}

/// Text that is cut at `max` characters but keeps its lines. The second value tells that it was cut.
pub fn cap(text: &str, max: usize) -> (String, bool) {
    if text.chars().count() <= max {
        return (text.to_owned(), false);
    }
    (text.chars().take(max).collect(), true)
}

#[cfg(test)]
mod tests;
