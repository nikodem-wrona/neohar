//! The items of the timeline of a run (`timeline:<run>`).
//!
//! The builder takes the entries of a transcript in the order of the file and gives the items to
//! publish. An item has a stable id, so a later entry can change it: the result of a tool call
//! changes the item of the call. Raw transcripts are never copied. The items hold previews and
//! summaries, and the tool outputs are cut at [`OUTPUT_CHARS`](super::OUTPUT_CHARS).

use std::collections::{HashMap, VecDeque};

use nh_proto::api::{
    model::{TimelineBody, TimelineItem, ToolStatus},
    time::Timestamp,
};

use super::{Block, Entry, Kind, OUTPUT_CHARS, PREVIEW_CHARS, cap, preview};

/// How many calls the builder waits for a result of. Older calls stay as they are.
const OPEN_CALLS: usize = 256;

#[derive(Debug)]
enum Call {
    Tool { tool: String, summary: String },
    Subagent { agent_type: String },
}

#[derive(Debug)]
struct Open {
    call: Call,
    at: Timestamp,
    turn: u32,
}

/// The state that turns entries into items.
#[derive(Debug, Default)]
pub struct Builder {
    turn: u32,
    /// A turn started (Codex) and nothing has come after it. The prompt belongs to this turn.
    fresh_turn: bool,
    open: HashMap<String, Open>,
    order: VecDeque<String>,
}

impl Builder {
    /// The number of the current turn. The first turn is 1.
    pub fn turn(&self) -> u32 {
        self.turn
    }

    /// The items that one entry makes or changes. An item with an id that was published before
    /// replaces that item.
    pub fn feed(&mut self, entry: &Entry, fallback_at: Timestamp) -> Vec<TimelineItem> {
        if entry.sidechain {
            return Vec::new();
        }
        let at = entry.at.unwrap_or(fallback_at);
        match &entry.kind {
            Kind::TurnStart => {
                self.turn += 1;
                self.fresh_turn = true;
                Vec::new()
            }
            Kind::Prompt { text } => {
                if self.fresh_turn {
                    self.fresh_turn = false;
                } else {
                    self.turn += 1;
                }
                vec![self.item(
                    entry.id.clone(),
                    at,
                    TimelineBody::Prompt {
                        preview: preview(text, PREVIEW_CHARS),
                    },
                )]
            }
            Kind::Assistant { blocks, .. } => {
                self.fresh_turn = false;
                let mut items = Vec::new();
                for (index, block) in blocks.iter().enumerate() {
                    match block {
                        Block::Text(text) => items.push(self.item(
                            format!("{}.{index}", entry.id),
                            at,
                            TimelineBody::Assistant {
                                preview: preview(text, PREVIEW_CHARS),
                            },
                        )),
                        Block::ToolUse {
                            id,
                            name,
                            summary,
                            subagent,
                        } => {
                            let (call, body) = match subagent {
                                Some(agent_type) => (
                                    Call::Subagent {
                                        agent_type: agent_type.clone(),
                                    },
                                    TimelineBody::Subagent {
                                        agent_id: id.clone(),
                                        agent_type: agent_type.clone(),
                                        finished: false,
                                    },
                                ),
                                None => (
                                    Call::Tool {
                                        tool: name.clone(),
                                        summary: summary.clone(),
                                    },
                                    TimelineBody::Tool {
                                        tool: name.clone(),
                                        status: ToolStatus::Running,
                                        summary: summary.clone(),
                                        duration_ms: None,
                                    },
                                ),
                            };
                            self.remember(id, call, at);
                            items.push(self.item(id.clone(), at, body));
                        }
                    }
                }
                items
            }
            Kind::ToolResult {
                tool_use,
                failed,
                denied,
                text,
            } => {
                let mut items = Vec::new();
                match self.forget(tool_use) {
                    Some(Open {
                        call: Call::Tool { tool, summary },
                        at: started,
                        turn,
                    }) => {
                        let status = if *denied {
                            ToolStatus::Denied
                        } else if *failed {
                            ToolStatus::Failed
                        } else {
                            ToolStatus::Ok
                        };
                        let duration = u64::try_from(at.0 - started.0).ok();
                        items.push(TimelineItem {
                            id: tool_use.clone(),
                            at: started,
                            turn,
                            body: TimelineBody::Tool {
                                tool,
                                status,
                                summary,
                                duration_ms: duration,
                            },
                        });
                    }
                    Some(Open {
                        call: Call::Subagent { agent_type },
                        at: started,
                        turn,
                    }) => {
                        items.push(TimelineItem {
                            id: tool_use.clone(),
                            at: started,
                            turn,
                            body: TimelineBody::Subagent {
                                agent_id: tool_use.clone(),
                                agent_type,
                                finished: true,
                            },
                        });
                        return items;
                    }
                    None => {}
                }
                if !text.trim().is_empty() {
                    let (text, truncated) = cap(text, OUTPUT_CHARS);
                    items.push(self.item(
                        format!("{tool_use}.out"),
                        at,
                        TimelineBody::Output { text, truncated },
                    ));
                }
                items
            }
            Kind::Compaction => vec![self.item(entry.id.clone(), at, TimelineBody::Compaction {})],
            _ => Vec::new(),
        }
    }

    fn item(&self, id: String, at: Timestamp, body: TimelineBody) -> TimelineItem {
        TimelineItem {
            id,
            at,
            turn: self.turn,
            body,
        }
    }

    fn remember(&mut self, id: &str, call: Call, at: Timestamp) {
        if self.open.len() >= OPEN_CALLS
            && let Some(oldest) = self.order.pop_front()
        {
            self.open.remove(&oldest);
        }
        self.order.push_back(id.to_owned());
        self.open.insert(
            id.to_owned(),
            Open {
                call,
                at,
                turn: self.turn,
            },
        );
    }

    fn forget(&mut self, id: &str) -> Option<Open> {
        let open = self.open.remove(id)?;
        self.order.retain(|queued| queued != id);
        Some(open)
    }
}
