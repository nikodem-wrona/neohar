//! A scenario: what the mock model says, in order.
//!
//! The file is TOML and not YAML: the workspace has no YAML parser, and `toml` is a dependency
//! already. A scenario has turns. A turn is a list of blocks, and a block is
//! text or a tool call.
//!
//! ```toml
//! default_reply = "ok"      # for calls that match no turn: titles, summaries
//! chunk_delay_ms = 0        # the time between two events of a stream
//!
//! [[turn]]
//! blocks = [{ type = "text", text = "Hello from the mock model." }]
//!
//! [[turn]]
//! match_user = "list the files"   # the last user message or tool result must contain this
//! blocks = [
//!   { type = "text", text = "I will look." },
//!   { type = "tool_use", name = "Bash", input = { command = "ls" } },
//! ]
//! ```
//!
//! A text or a string in a tool input can hold `{{find:REGEX}}`. The mock replaces it with the first
//! match of the regular expression in the text of the request (the JSON, as the harness sent it).
//! A scenario uses it for a value that the harness picks at random and that the model must repeat,
//! such as the path of the plan file in the system prompt of Claude Code:
//! `{ type = "tool_use", name = "Write", input = { file_path = "{{find:/[^\" ]+/plans/[a-z0-9-]+\\.md}}", content = "..." } }`.
//! A pattern without a match stays as it is, and the log says so (`expansion_failures`).
//!
//! A request is **main** when it offers tools, and **auxiliary** otherwise. The harnesses make
//! auxiliary calls on the side: a title, a summary, a check of the quota. Only main requests use
//! up turns. A main request gets the first turn that no request used yet and whose `match_user`
//! (if there is one) is in the last user message or tool result. An auxiliary request, and a
//! main request that no turn fits, get `default_reply`. The log says which one happened.

use serde::Deserialize;
use serde_json::Value;

/// The text for a request that no turn fits.
const DEFAULT_REPLY: &str = "OK.";

fn default_reply() -> String {
    DEFAULT_REPLY.to_owned()
}

/// What the model says in a turn.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Block {
    Text {
        text: String,
    },
    /// A call of a tool of the harness. `input` is the argument object.
    ToolUse {
        name: String,
        input: Value,
    },
}

/// One reply of the model to a main request.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Turn {
    /// A text that the last user message or tool result must contain for this turn to fit.
    pub match_user: Option<String>,
    pub blocks: Vec<Block>,
    /// The tokens that the reply reports. Without them, the mock counts four characters for one
    /// token.
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}

/// The whole script of the model.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    #[serde(default = "default_reply")]
    pub default_reply: String,
    /// The time between two events of a stream, in milliseconds. 0 sends them at once.
    #[serde(default)]
    pub chunk_delay_ms: u64,
    #[serde(default, rename = "turn")]
    pub turns: Vec<Turn>,
}

/// Why a scenario does not load.
#[derive(Debug, thiserror::Error)]
pub enum ScenarioError {
    #[error("the scenario is not valid TOML: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("turn {turn} has no blocks")]
    EmptyTurn { turn: usize },
    #[error("turn {turn}: a text block is empty")]
    EmptyText { turn: usize },
    #[error("turn {turn}: a tool name is empty")]
    EmptyToolName { turn: usize },
    #[error("turn {turn}: the tool input of {name:?} is not an object")]
    ToolInputNotObject { turn: usize, name: String },
}

impl Scenario {
    /// A scenario with no turns: every request gets the default reply.
    pub fn empty() -> Self {
        Self {
            default_reply: default_reply(),
            chunk_delay_ms: 0,
            turns: Vec::new(),
        }
    }

    /// Reads a scenario from TOML and checks it.
    pub fn parse(text: &str) -> Result<Self, ScenarioError> {
        let scenario: Self = toml::from_str(text)?;
        for (turn, t) in scenario.turns.iter().enumerate() {
            if t.blocks.is_empty() {
                return Err(ScenarioError::EmptyTurn { turn });
            }
            for block in &t.blocks {
                match block {
                    Block::Text { text } if text.is_empty() => {
                        return Err(ScenarioError::EmptyText { turn });
                    }
                    Block::ToolUse { name, .. } if name.is_empty() => {
                        return Err(ScenarioError::EmptyToolName { turn });
                    }
                    Block::ToolUse { name, input } if !input.is_object() => {
                        return Err(ScenarioError::ToolInputNotObject {
                            turn,
                            name: name.clone(),
                        });
                    }
                    _ => {}
                }
            }
        }
        Ok(scenario)
    }
}

/// What the model answers to one request.
#[derive(Clone, Debug, PartialEq)]
pub struct Reply {
    pub blocks: Vec<Block>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    /// The index of the turn that this reply is, or `None` for the default reply.
    pub turn: Option<usize>,
}

/// Replaces the `{{find:REGEX}}` markers of `text`. A marker that cannot be replaced stays, and its
/// pattern goes to `failures`.
fn expand_text(text: &str, request: &str, failures: &mut Vec<String>) -> String {
    const OPEN: &str = "{{find:";
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(OPEN) {
        out.push_str(&rest[..start]);
        let after = &rest[start + OPEN.len()..];
        let Some(end) = after.find("}}") else {
            out.push_str(&rest[start..]);
            return out;
        };
        let pattern = &after[..end];
        match regex::Regex::new(pattern)
            .ok()
            .and_then(|re| re.find(request).map(|m| m.as_str().to_owned()))
        {
            Some(found) => out.push_str(&found),
            None => {
                failures.push(pattern.to_owned());
                out.push_str(&rest[start..start + OPEN.len() + end + 2]);
            }
        }
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    out
}

fn expand_value(value: &mut Value, request: &str, failures: &mut Vec<String>) {
    match value {
        Value::String(text) => *text = expand_text(text, request, failures),
        Value::Array(items) => items
            .iter_mut()
            .for_each(|item| expand_value(item, request, failures)),
        Value::Object(map) => map
            .values_mut()
            .for_each(|item| expand_value(item, request, failures)),
        _ => {}
    }
}

impl Reply {
    /// Replaces the `{{find:REGEX}}` markers in the blocks. It returns the patterns that did not
    /// match.
    pub fn expand(&mut self, request: &str) -> Vec<String> {
        let mut failures = Vec::new();
        for block in &mut self.blocks {
            match block {
                Block::Text { text } => *text = expand_text(text, request, &mut failures),
                Block::ToolUse { input, .. } => expand_value(input, request, &mut failures),
            }
        }
        failures
    }

    /// `end_turn`, or `tool_use` when the reply calls a tool (the Anthropic names).
    pub fn calls_tool(&self) -> bool {
        self.blocks
            .iter()
            .any(|block| matches!(block, Block::ToolUse { .. }))
    }
}

/// Hands out the turns of a scenario, each at most once.
pub struct Dispatcher {
    scenario: Scenario,
    used: Vec<bool>,
}

impl Dispatcher {
    pub fn new(scenario: Scenario) -> Self {
        let used = vec![false; scenario.turns.len()];
        Self { scenario, used }
    }

    pub fn chunk_delay_ms(&self) -> u64 {
        self.scenario.chunk_delay_ms
    }

    /// The reply to a request. `main` is whether the request offers tools. `last_user` is the text
    /// of the last user message or tool result.
    pub fn pick(&mut self, main: bool, last_user: &str) -> Reply {
        if main {
            let found = self
                .scenario
                .turns
                .iter()
                .enumerate()
                .find(|(index, turn)| {
                    !self.used[*index]
                        && turn
                            .match_user
                            .as_deref()
                            .is_none_or(|needle| last_user.contains(needle))
                });
            if let Some((index, turn)) = found {
                self.used[index] = true;
                return Reply {
                    blocks: turn.blocks.clone(),
                    input_tokens: turn.input_tokens,
                    output_tokens: turn.output_tokens,
                    turn: Some(index),
                };
            }
        }
        Reply {
            blocks: vec![Block::Text {
                text: self.scenario.default_reply.clone(),
            }],
            input_tokens: None,
            output_tokens: None,
            turn: None,
        }
    }

    /// How many turns requests used, and how many the scenario has.
    pub fn progress(&self) -> (usize, usize) {
        (self.used.iter().filter(|u| **u).count(), self.used.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCENARIO: &str = r#"
default_reply = "fallback"

[[turn]]
blocks = [{ type = "text", text = "first" }]

[[turn]]
match_user = "list"
blocks = [
  { type = "text", text = "looking" },
  { type = "tool_use", name = "Bash", input = { command = "ls" } },
]

[[turn]]
blocks = [{ type = "text", text = "third" }]
output_tokens = 7
"#;

    fn text_of(reply: &Reply) -> String {
        match &reply.blocks[0] {
            Block::Text { text } => text.clone(),
            other => panic!("not text: {other:?}"),
        }
    }

    #[test]
    fn a_scenario_loads_with_its_turns_and_defaults() {
        let scenario = Scenario::parse(SCENARIO).expect("a good scenario");
        assert_eq!(scenario.turns.len(), 3);
        assert_eq!(scenario.default_reply, "fallback");
        assert_eq!(scenario.chunk_delay_ms, 0);
        assert_eq!(scenario.turns[2].output_tokens, Some(7));
        assert_eq!(
            scenario.turns[1].blocks[1],
            Block::ToolUse {
                name: "Bash".into(),
                input: serde_json::json!({ "command": "ls" })
            }
        );
        assert_eq!(Scenario::empty().default_reply, "OK.");
    }

    #[test]
    fn mistakes_in_a_scenario_are_errors_that_name_the_turn() {
        let bad = |text: &str| {
            Scenario::parse(text)
                .expect_err("a bad scenario")
                .to_string()
        };
        assert!(bad("[[turn]]\nblocks = []").contains("turn 0 has no blocks"));
        assert!(bad("[[turn]]\nblocks = [{ type = \"text\", text = \"\" }]").contains("empty"));
        assert!(
            bad("[[turn]]\nblocks = [{ type = \"tool_use\", name = \"\", input = {} }]")
                .contains("tool name")
        );
        assert!(
            bad("[[turn]]\nblocks = [{ type = \"tool_use\", name = \"X\", input = 3 }]")
                .contains("not an object")
        );
        assert!(bad("surprise = 1").contains("surprise"));
        assert!(bad("[[turn]]\nblocks = [{ type = \"image\" }]").contains("image"));
    }

    #[test]
    fn main_requests_take_the_turns_in_order_and_each_turn_once() {
        let mut d = Dispatcher::new(Scenario::parse(SCENARIO).expect("a good scenario"));
        // Turn 1 wants "list", so the first request, which says something else, gets turn 0.
        assert_eq!(text_of(&d.pick(true, "hello")), "first");
        // "hello" does not fit turn 1, so the next request gets turn 2.
        let third = d.pick(true, "hello again");
        assert_eq!(
            (text_of(&third), third.turn, third.output_tokens),
            ("third".into(), Some(2), Some(7))
        );
        // Nothing but turn 1 is left, and it needs "list".
        assert_eq!(d.pick(true, "something").turn, None);
        let second = d.pick(true, "please list the files");
        assert_eq!(second.turn, Some(1));
        assert!(second.calls_tool());
        assert_eq!(d.pick(true, "list").turn, None, "a turn is used once");
        assert_eq!(d.progress(), (3, 3));
    }

    #[test]
    fn find_markers_take_a_value_from_the_request() {
        let mut reply = Reply {
            blocks: vec![
                Block::Text {
                    text: "plan at {{find:/tmp/[a-z/-]+\\.md}} done".into(),
                },
                Block::ToolUse {
                    name: "Write".into(),
                    input: serde_json::json!({
                        "file_path": "{{find:/tmp/[a-z/-]+\\.md}}",
                        "nested": ["{{find:id-[0-9]+}}", 3],
                    }),
                },
            ],
            input_tokens: None,
            output_tokens: None,
            turn: Some(0),
        };
        let failures = reply.expand("... create your plan at /tmp/a/b-c.md using ... id-42 ...");
        assert!(failures.is_empty(), "{failures:?}");
        assert_eq!(
            reply.blocks[0],
            Block::Text {
                text: "plan at /tmp/a/b-c.md done".into()
            }
        );
        assert_eq!(
            reply.blocks[1],
            Block::ToolUse {
                name: "Write".into(),
                input: serde_json::json!({ "file_path": "/tmp/a/b-c.md", "nested": ["id-42", 3] })
            }
        );
    }

    #[test]
    fn a_marker_without_a_match_stays_and_is_reported() {
        let mut reply = Reply {
            blocks: vec![Block::Text {
                text: "a {{find:zzz+}} b {{find:(}} c {{find:open".into(),
            }],
            input_tokens: None,
            output_tokens: None,
            turn: None,
        };
        let failures = reply.expand("nothing here");
        assert_eq!(failures, ["zzz+", "("]);
        assert_eq!(
            reply.blocks[0],
            Block::Text {
                text: "a {{find:zzz+}} b {{find:(}} c {{find:open".into()
            }
        );
    }

    #[test]
    fn an_auxiliary_request_gets_the_default_and_uses_no_turn() {
        let mut d = Dispatcher::new(Scenario::parse(SCENARIO).expect("a good scenario"));
        let reply = d.pick(false, "write a title");
        assert_eq!((text_of(&reply), reply.turn), ("fallback".into(), None));
        assert_eq!(d.progress(), (0, 3));
    }
}
