//! The items of one chat, from the ACP messages of Hermes. No I/O.
//!
//! The core keeps no copy of the conversation: a resume of the session sends the history again,
//! and [`Thread::reset`] starts the list again before it. The rules come from the spike of
//! 2026-10-31 (`docs/plans/hermes-chat.md`):
//!
//! - The `messageId` of Hermes is different for each client and a replay has none, so the items
//!   have keys of their own, in the order of arrival.
//! - A client that did not send the prompt gets the deltas of a message and then one chunk with
//!   the full text. A chunk that starts with the text of the message replaces it.
//! - An approval has a tool call of its own (`edit-approval-N`) that only `tool_call_update` names.
//!   The chat shows the approval, not that call.

use std::collections::BTreeSet;

use nh_proto::api::{
    model::{
        HermesAnswerState, HermesItem, HermesItemBody, HermesOption, HermesPlanEntry,
        HermesPlanStatus, HermesToolStatus,
    },
    time::Timestamp,
};
use serde_json::Value;

/// The longest text of one item that the core renders.
const TEXT_CAP: usize = 256 * 1024;

/// A file change that a tool or an approval shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Change {
    pub path: String,
    pub old: String,
    pub new: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Kind {
    Prompt {
        text: String,
    },
    Assistant {
        text: String,
        message: Option<String>,
    },
    Thought {
        text: String,
    },
    Tool {
        id: String,
        title: String,
        kind: String,
        status: HermesToolStatus,
        output: String,
        change: Option<Change>,
    },
    Plan {
        entries: Vec<HermesPlanEntry>,
    },
    Approval {
        request: String,
        /// The JSON-RPC id of the request, for the answer.
        rpc: Value,
        title: String,
        detail: String,
        change: Option<Change>,
        options: Vec<HermesOption>,
        state: HermesAnswerState,
        answer: Option<String>,
        deadline: i64,
    },
    Notice {
        text: String,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Entry {
    pub key: String,
    pub at: i64,
    pub kind: Kind,
}

/// What an update told, beyond the items.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Outcome {
    pub title: Option<String>,
    pub context: Option<(u64, u64)>,
    pub mode: Option<String>,
    /// Text of the agent arrived.
    pub agent_text: bool,
    /// A tool call started or changed.
    pub tool: bool,
    /// A usage update arrived. Hermes sends one after each step of a turn.
    pub usage: bool,
}

/// The changes since the last [`Thread::take_changes`].
#[derive(Debug, Default)]
pub(crate) struct Changes {
    pub removed: Vec<String>,
    pub items: Vec<HermesItem>,
    /// The order of the keys changed: a new item or a reset.
    pub order: Option<Vec<String>>,
}

#[derive(Debug, Default)]
pub(crate) struct Thread {
    entries: Vec<Entry>,
    next_key: u64,
    dirty: BTreeSet<usize>,
    removed: Vec<String>,
    reordered: bool,
}

/// Why an answer was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AnswerError {
    NoSuchRequest,
    NotOpen(HermesAnswerState),
    NoSuchOption,
}

impl Thread {
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Starts the list again, before a resume sends the history.
    pub fn reset(&mut self) {
        self.removed
            .extend(self.entries.drain(..).map(|entry| entry.key));
        self.dirty.clear();
        self.reordered = true;
    }

    fn push(&mut self, at: i64, kind: Kind) -> usize {
        self.next_key += 1;
        self.entries.push(Entry {
            key: self.next_key.to_string(),
            at,
            kind,
        });
        self.reordered = true;
        let index = self.entries.len() - 1;
        self.dirty.insert(index);
        index
    }

    fn touch(&mut self, index: usize) {
        self.dirty.insert(index);
    }

    /// A prompt that neohar sent. Hermes does not send it back to its sender.
    pub fn prompt(&mut self, text: &str, at: i64) {
        self.push(
            at,
            Kind::Prompt {
                text: text.to_owned(),
            },
        );
    }

    pub fn notice(&mut self, text: &str, at: i64) {
        self.push(
            at,
            Kind::Notice {
                text: text.to_owned(),
            },
        );
    }

    /// Marks the last item changed: its `streaming` flag follows the state of the chat.
    pub fn touch_last(&mut self) {
        if let Some(index) = self.entries.len().checked_sub(1) {
            self.touch(index);
        }
    }

    /// Whether the history ends in an open turn: a prompt with no answer, or a tool that runs.
    pub fn ends_open(&self) -> bool {
        let tool_runs = self.entries.iter().any(|entry| {
            matches!(
                entry.kind,
                Kind::Tool {
                    status: HermesToolStatus::Running,
                    ..
                }
            )
        });
        tool_runs
            || matches!(
                self.entries.last().map(|e| &e.kind),
                Some(Kind::Prompt { .. })
            )
    }

    /// Marks the tools that still run as failed: the turn ended.
    pub fn end_turn(&mut self) {
        let mut changed = Vec::new();
        for (index, entry) in self.entries.iter_mut().enumerate() {
            if let Kind::Tool { status, .. } = &mut entry.kind
                && *status == HermesToolStatus::Running
            {
                *status = HermesToolStatus::Failed;
                changed.push(index);
            }
        }
        for index in changed {
            self.touch(index);
        }
        self.touch_last();
    }

    /// Applies one `session/update`.
    pub fn update(&mut self, update: &Value, at: i64) -> Outcome {
        let mut outcome = Outcome::default();
        let kind = update
            .get("sessionUpdate")
            .and_then(Value::as_str)
            .unwrap_or("");
        match kind {
            "user_message_chunk" => {
                if let Some(text) = chunk_text(update) {
                    self.append_text(at, Text::Prompt, text, None);
                }
            }
            "agent_message_chunk" => {
                if let Some(text) = chunk_text(update) {
                    let message = update
                        .get("messageId")
                        .and_then(Value::as_str)
                        .map(str::to_owned);
                    self.append_text(at, Text::Assistant, text, message);
                    outcome.agent_text = true;
                }
            }
            "agent_thought_chunk" => {
                if let Some(text) = chunk_text(update) {
                    self.append_text(at, Text::Thought, text, None);
                    outcome.agent_text = true;
                }
            }
            "tool_call" | "tool_call_update" => {
                outcome.tool = self.tool(update, at, kind == "tool_call");
            }
            "plan" => self.plan(update, at),
            "usage_update" => {
                outcome.usage = true;
                let used = update.get("used").and_then(Value::as_u64);
                let size = update.get("size").and_then(Value::as_u64);
                if let (Some(used), Some(size)) = (used, size) {
                    outcome.context = Some((used, size));
                }
            }
            "session_info_update" => {
                outcome.title = update
                    .get("title")
                    .and_then(Value::as_str)
                    .filter(|t| !t.trim().is_empty())
                    .map(str::to_owned);
            }
            "current_mode_update" => {
                outcome.mode = update
                    .get("currentModeId")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
            }
            _ => {}
        }
        outcome
    }

    fn append_text(&mut self, at: i64, which: Text, chunk: &str, message: Option<String>) {
        if let Some(index) = self.entries.len().checked_sub(1) {
            let entry = &mut self.entries[index];
            let target = match (&mut entry.kind, which) {
                (Kind::Prompt { text }, Text::Prompt) | (Kind::Thought { text }, Text::Thought) => {
                    Some(text)
                }
                (
                    Kind::Assistant {
                        text,
                        message: current,
                    },
                    Text::Assistant,
                ) if current.is_none() || message.is_none() || *current == message => {
                    if current.is_none() {
                        current.clone_from(&message);
                    }
                    Some(text)
                }
                _ => None,
            };
            if let Some(text) = target {
                if !text.is_empty() && chunk.starts_with(text.as_str()) {
                    // The full text again (see the module comment).
                    chunk.clone_into(text);
                } else if text.len() < TEXT_CAP {
                    text.push_str(chunk);
                }
                self.touch(index);
                return;
            }
        }
        let text = chunk.to_owned();
        let kind = match which {
            Text::Prompt => Kind::Prompt { text },
            Text::Assistant => Kind::Assistant { text, message },
            Text::Thought => Kind::Thought { text },
        };
        self.push(at, kind);
    }

    fn tool(&mut self, update: &Value, at: i64, start: bool) -> bool {
        let Some(id) = update.get("toolCallId").and_then(Value::as_str) else {
            return false;
        };
        let title = update.get("title").and_then(Value::as_str);
        let found = self
            .entries
            .iter()
            .position(|entry| matches!(&entry.kind, Kind::Tool { id: known, .. } if known == id));
        let index = match found {
            Some(index) => index,
            // An update of a call that the chat does not know, without a title: the tool call of
            // an approval. The approval card shows it.
            None if !start && title.is_none() => return false,
            None => self.push(
                at,
                Kind::Tool {
                    id: id.to_owned(),
                    title: String::new(),
                    kind: "other".to_owned(),
                    status: HermesToolStatus::Running,
                    output: String::new(),
                    change: None,
                },
            ),
        };
        let (texts, change) = content(update);
        if let Kind::Tool {
            title: current_title,
            kind,
            status,
            output,
            change: current_change,
            ..
        } = &mut self.entries[index].kind
        {
            if let Some(title) = title {
                title.clone_into(current_title);
            }
            if let Some(value) = update.get("kind").and_then(Value::as_str) {
                value.clone_into(kind);
            }
            if let Some(value) = update.get("status").and_then(Value::as_str) {
                *status = tool_status(value);
            }
            if !texts.is_empty() {
                *output = cap(texts.join("\n\n"));
            }
            if change.is_some() {
                *current_change = change;
            }
        }
        self.touch(index);
        true
    }

    fn plan(&mut self, update: &Value, at: i64) {
        let entries: Vec<HermesPlanEntry> = update
            .get("entries")
            .and_then(Value::as_array)
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(|entry| {
                        Some(HermesPlanEntry {
                            text: entry.get("content")?.as_str()?.to_owned(),
                            status: match entry.get("status").and_then(Value::as_str) {
                                Some("completed") => HermesPlanStatus::Completed,
                                Some("in_progress") => HermesPlanStatus::InProgress,
                                _ => HermesPlanStatus::Pending,
                            },
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        // The plan of the current turn: after the last prompt.
        let turn_start = self
            .entries
            .iter()
            .rposition(|entry| matches!(entry.kind, Kind::Prompt { .. }))
            .map_or(0, |index| index + 1);
        let found = self.entries[turn_start..]
            .iter()
            .rposition(|entry| matches!(entry.kind, Kind::Plan { .. }))
            .map(|offset| turn_start + offset);
        match found {
            Some(index) => {
                self.entries[index].kind = Kind::Plan { entries };
                self.touch(index);
            }
            None => {
                self.push(at, Kind::Plan { entries });
            }
        }
    }

    /// A `session/request_permission`. Returns the key of the request that `hermes.answer`
    /// names.
    pub fn approval(&mut self, rpc: Value, params: &Value, at: i64, deadline: i64) -> String {
        let call = params.get("toolCall").cloned().unwrap_or(Value::Null);
        let request = call
            .get("toolCallId")
            .and_then(Value::as_str)
            .map_or_else(|| format!("rpc-{rpc}"), str::to_owned);
        let title = call
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("Approve a tool call")
            .to_owned();
        let (texts, change) = content(&call);
        let options = params
            .get("options")
            .and_then(Value::as_array)
            .map(|options| {
                options
                    .iter()
                    .filter_map(|option| {
                        let id = option.get("optionId")?.as_str()?.to_owned();
                        let label = option
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or(&id)
                            .to_owned();
                        let allow = option
                            .get("kind")
                            .and_then(Value::as_str)
                            .is_some_and(|kind| kind.starts_with("allow"));
                        Some(HermesOption { id, label, allow })
                    })
                    .collect()
            })
            .unwrap_or_default();
        let kind = Kind::Approval {
            request: request.clone(),
            rpc,
            title,
            detail: cap(texts.join("\n\n")),
            change,
            options,
            state: HermesAnswerState::Open,
            answer: None,
            deadline,
        };
        let open = self.entries.iter().position(|entry| {
            matches!(&entry.kind, Kind::Approval { request: r, state: HermesAnswerState::Open, .. } if *r == request)
        });
        match open {
            // The same request again, after a new connection: the new JSON-RPC id counts.
            Some(index) => {
                self.entries[index].kind = kind;
                self.touch(index);
            }
            None => {
                self.push(at, kind);
            }
        }
        request
    }

    /// Records an answer. Returns the JSON-RPC id and the option to send.
    pub fn answer(&mut self, request: &str, option: &str) -> Result<(Value, String), AnswerError> {
        let index = self
            .entries
            .iter()
            .rposition(
                |entry| matches!(&entry.kind, Kind::Approval { request: r, .. } if r == request),
            )
            .ok_or(AnswerError::NoSuchRequest)?;
        let Kind::Approval {
            rpc,
            options,
            state,
            answer,
            ..
        } = &mut self.entries[index].kind
        else {
            return Err(AnswerError::NoSuchRequest);
        };
        if *state != HermesAnswerState::Open {
            return Err(AnswerError::NotOpen(*state));
        }
        let picked = options
            .iter()
            .find(|o| o.id == option)
            .ok_or(AnswerError::NoSuchOption)?;
        *answer = Some(picked.label.clone());
        *state = HermesAnswerState::Answered;
        let reply = (rpc.clone(), picked.id.clone());
        self.touch(index);
        Ok(reply)
    }

    /// Ends the open approvals with `state`. Returns their JSON-RPC ids.
    pub fn end_approvals(&mut self, end: HermesAnswerState) -> Vec<Value> {
        let mut ids = Vec::new();
        let mut changed = Vec::new();
        for (index, entry) in self.entries.iter_mut().enumerate() {
            if let Kind::Approval { rpc, state, .. } = &mut entry.kind
                && *state == HermesAnswerState::Open
            {
                *state = end;
                ids.push(rpc.clone());
                changed.push(index);
            }
        }
        for index in changed {
            self.touch(index);
        }
        ids
    }

    /// Marks the open approvals whose deadline went by as expired. Says whether one changed.
    pub fn expire(&mut self, now: i64) -> bool {
        let mut changed = Vec::new();
        for (index, entry) in self.entries.iter_mut().enumerate() {
            if let Kind::Approval {
                state, deadline, ..
            } = &mut entry.kind
                && *state == HermesAnswerState::Open
                && *deadline <= now
            {
                *state = HermesAnswerState::Expired;
                changed.push(index);
            }
        }
        let any = !changed.is_empty();
        for index in changed {
            self.touch(index);
        }
        any
    }

    pub fn open_approvals(&self) -> u32 {
        let open = self
            .entries
            .iter()
            .filter(|entry| {
                matches!(
                    entry.kind,
                    Kind::Approval {
                        state: HermesAnswerState::Open,
                        ..
                    }
                )
            })
            .count();
        u32::try_from(open).unwrap_or(u32::MAX)
    }

    pub fn has_changes(&self) -> bool {
        !self.dirty.is_empty() || !self.removed.is_empty() || self.reordered
    }

    /// Renders the items that changed. `streaming`: the last item can still grow.
    pub fn take_changes(&mut self, streaming: bool) -> Changes {
        let last = self.entries.len().checked_sub(1);
        let items = std::mem::take(&mut self.dirty)
            .into_iter()
            .filter_map(|index| {
                let entry = self.entries.get(index)?;
                Some(render(entry, streaming && Some(index) == last))
            })
            .collect();
        let order = std::mem::take(&mut self.reordered)
            .then(|| self.entries.iter().map(|e| e.key.clone()).collect());
        Changes {
            removed: std::mem::take(&mut self.removed),
            items,
            order,
        }
    }
}

#[derive(Clone, Copy)]
enum Text {
    Prompt,
    Assistant,
    Thought,
}

fn chunk_text(update: &Value) -> Option<&str> {
    let content = update.get("content")?;
    (content.get("type").and_then(Value::as_str) == Some("text"))
        .then(|| content.get("text").and_then(Value::as_str))
        .flatten()
}

/// The texts and the first file change of the `content` of a tool call.
fn content(call: &Value) -> (Vec<String>, Option<Change>) {
    let mut texts = Vec::new();
    let mut change = None;
    for block in call
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        match block.get("type").and_then(Value::as_str) {
            Some("content") => {
                if let Some(text) = block
                    .get("content")
                    .filter(|c| c.get("type").and_then(Value::as_str) == Some("text"))
                    .and_then(|c| c.get("text"))
                    .and_then(Value::as_str)
                {
                    texts.push(text.to_owned());
                }
            }
            Some("diff") if change.is_none() => {
                let path = block.get("path").and_then(Value::as_str).unwrap_or("");
                change = Some(Change {
                    path: path.to_owned(),
                    old: block
                        .get("oldText")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_owned(),
                    new: block
                        .get("newText")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_owned(),
                });
            }
            _ => {}
        }
    }
    (texts, change)
}

fn tool_status(value: &str) -> HermesToolStatus {
    match value {
        "completed" => HermesToolStatus::Done,
        "failed" => HermesToolStatus::Failed,
        _ => HermesToolStatus::Running,
    }
}

fn cap(mut text: String) -> String {
    if text.len() > TEXT_CAP {
        let mut end = TEXT_CAP;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
    }
    text
}

fn markdown(text: &str) -> nh_proto::api::rich::Markdown {
    crate::rich_calls::markdown(&nh_rich::markdown(text))
}

fn diff(change: &Change) -> nh_proto::api::rich::Diff {
    let path = (!change.path.is_empty()).then_some(change.path.as_str());
    crate::rich_calls::diff(&nh_rich::diff::between(
        &change.old,
        &change.new,
        path,
        None,
    ))
}

fn render(entry: &Entry, streaming: bool) -> HermesItem {
    let body = match &entry.kind {
        Kind::Prompt { text } => HermesItemBody::Prompt {
            markdown: markdown(text),
        },
        Kind::Assistant { text, .. } => HermesItemBody::Assistant {
            markdown: markdown(text),
            streaming,
        },
        Kind::Thought { text } => HermesItemBody::Thought {
            markdown: markdown(text),
        },
        Kind::Tool {
            title,
            kind,
            status,
            output,
            change,
            ..
        } => HermesItemBody::Tool {
            title: title.clone(),
            kind: kind.clone(),
            status: *status,
            output: (!output.is_empty()).then(|| markdown(output)),
            diff: change.as_ref().map(diff),
        },
        Kind::Plan { entries } => HermesItemBody::Plan {
            entries: entries.clone(),
        },
        Kind::Approval {
            request,
            title,
            detail,
            change,
            options,
            state,
            answer,
            deadline,
            ..
        } => HermesItemBody::Approval {
            request: request.clone(),
            title: title.clone(),
            detail: (!detail.is_empty()).then(|| markdown(detail)),
            diff: change.as_ref().map(diff),
            options: options.clone(),
            state: *state,
            answer: answer.clone(),
            deadline: Some(Timestamp(*deadline)),
        },
        Kind::Notice { text } => HermesItemBody::Notice { text: text.clone() },
    };
    HermesItem {
        id: entry.key.clone(),
        at: Timestamp(entry.at),
        body,
    }
}
