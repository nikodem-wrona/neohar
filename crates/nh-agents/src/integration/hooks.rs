//! The hooks of neohar in the settings of a harness.
//!
//! Claude Code (`settings.json`) and Codex (`hooks.json`) keep their hooks in the same shape, and
//! the installer touches only the objects that are neohar's:
//!
//! ```json
//! { "hooks": { "PreToolUse": [ { "matcher": "*", "hooks": [
//!     { "type": "command", "command": "~/.local/bin/nh hook claude PreToolUse",
//!       "timeout": 1800 } ] } ] } }
//! ```
//!
//! A [`Spec`] says what differs: the harness and its events with their matchers and timeouts. Each
//! event gets one group of its own with one hook. A hook of neohar is told from a hook of the user
//! by its command, `<nh> hook <harness> <Event>`. A user can have groups and hooks of his
//! or her own in the same arrays. The installer leaves them as they are, also when they stand
//! beside a hook of neohar in the same group (it then removes only the hook of neohar).

use super::{
    Error, Result,
    json::{self, Edit, Node, indent_unit, line_indent, push, reindent, remove, replace},
};

/// One hook event of a harness.
pub struct Event {
    pub name: &'static str,
    /// The matcher of the group. Events without one match always.
    pub matcher: Option<&'static str>,
    /// The timeout of the hook in seconds. A hook without one gets the default of the harness.
    pub timeout: Option<u32>,
}

/// What differs between the harnesses.
pub struct Spec {
    /// `claude` or `codex`: the word after `hook` in the command.
    pub harness: &'static str,
    pub events: &'static [Event],
}

/// The command of a hook: the binary, quoted when its path needs it.
fn command(spec: &Spec, bin: &str, name: &str) -> String {
    let plain = bin
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'.' | b'_' | b'-' | b'~' | b'+'));
    let harness = spec.harness;
    if plain {
        format!("{bin} hook {harness} {name}")
    } else {
        format!(
            "\"{}\" hook {harness} {name}",
            bin.replace('\\', "\\\\").replace('"', "\\\"")
        )
    }
}

fn quote(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_else(|_| "\"\"".to_owned())
}

/// The group of an event, indented from zero with `unit`.
pub fn group_text(spec: &Spec, event: &Event, bin: &str, unit: &str) -> String {
    let mut lines = vec!["{".to_owned()];
    if let Some(matcher) = event.matcher {
        lines.push(format!("{unit}\"matcher\": {},", quote(matcher)));
    }
    lines.push(format!("{unit}\"hooks\": ["));
    lines.push(format!("{unit}{unit}{{"));
    let inner = format!("{unit}{unit}{unit}");
    lines.push(format!("{inner}\"type\": \"command\","));
    lines.push(format!(
        "{inner}\"command\": {}{}",
        quote(&command(spec, bin, event.name)),
        if event.timeout.is_some() { "," } else { "" }
    ));
    if let Some(timeout) = event.timeout {
        lines.push(format!("{inner}\"timeout\": {timeout}"));
    }
    lines.push(format!("{unit}{unit}}}"));
    lines.push(format!("{unit}]"));
    lines.push("}".to_owned());
    lines.join("\n")
}

/// The member of an event, `"Stop": [ group ]`, indented from zero.
fn event_member(spec: &Spec, event: &Event, bin: &str, unit: &str) -> String {
    format!(
        "\"{}\": [\n{unit}{}\n]",
        event.name,
        reindent(&group_text(spec, event, bin, unit), unit)
    )
}

/// The member `"hooks"` with every event, indented from zero.
fn hooks_member(spec: &Spec, bin: &str, unit: &str) -> String {
    let members: Vec<String> = spec
        .events
        .iter()
        .map(|event| {
            format!(
                "{unit}{}",
                reindent(&event_member(spec, event, bin, unit), unit)
            )
        })
        .collect();
    format!("\"hooks\": {{\n{}\n}}", members.join(",\n"))
}

/// Whether a command is the hook of neohar for this harness: `<nh> hook <harness> <Event>`.
fn is_hook_command(spec: &Spec, command: &str) -> bool {
    let Some((head, name)) = command.rsplit_once(' ') else {
        return false;
    };
    let suffix = format!(" hook {}", spec.harness);
    spec.events.iter().any(|event| event.name == name)
        && head.len() > suffix.len()
        && head.ends_with(&suffix)
}

/// Whether a hook object is neohar's: its command is the hook of neohar. An older neohar also
/// wrote a key `"_neohar"` in the hooks of Claude Code, always with this command and with the
/// same events, so the command finds those hooks too.
fn is_ours(spec: &Spec, hook: &Node) -> bool {
    hook.get("command")
        .and_then(|member| member.value.as_str())
        .is_some_and(|command| is_hook_command(spec, command))
}

/// What a group of an event array is.
enum Group {
    /// Not a group, or a group of the user with no hook of neohar.
    User,
    /// Every hook of the group is ours.
    Ours,
    /// Hooks of neohar beside hooks of the user. The indices are those of ours in `hooks`.
    Mixed(Vec<usize>),
}

fn classify(spec: &Spec, group: &Node) -> Group {
    let Some(hooks) = group.get("hooks").and_then(|m| m.value.elements()) else {
        return Group::User;
    };
    let ours: Vec<usize> = hooks
        .iter()
        .enumerate()
        .filter(|(_, hook)| is_ours(spec, hook))
        .map(|(i, _)| i)
        .collect();
    if ours.is_empty() {
        Group::User
    } else if ours.len() == hooks.len() {
        Group::Ours
    } else {
        Group::Mixed(ours)
    }
}

fn root_object(doc: &str) -> Result<Node> {
    let root = json::parse(doc)?;
    if root.members().is_none() {
        return Err(Error::Format(
            "the settings are not a JSON object, so neohar does not edit them".to_owned(),
        ));
    }
    Ok(root)
}

/// The next edit that brings the text nearer to the hooks of neohar, or `None` when it has them.
fn install_step(spec: &Spec, doc: &str, root: &Node, bin: &str) -> Result<Option<Edit>> {
    let unit = indent_unit(doc, root);
    let Some(hooks) = root.get("hooks") else {
        return Ok(Some(push(
            doc,
            root,
            &hooks_member(spec, bin, &unit),
            &unit,
        )));
    };
    if hooks.value.members().is_none() {
        return Err(Error::Format(
            "\"hooks\" in the settings is not an object, so neohar does not edit it".to_owned(),
        ));
    }
    for event in spec.events {
        let Some(member) = hooks.value.get(event.name) else {
            return Ok(Some(push(
                doc,
                &hooks.value,
                &event_member(spec, event, bin, &unit),
                &unit,
            )));
        };
        let Some(groups) = member.value.elements() else {
            return Err(Error::Format(format!(
                "\"{}\" in the hooks is not an array, so neohar does not edit it",
                event.name
            )));
        };
        let mut ours = Vec::new();
        for (index, group) in groups.iter().enumerate() {
            match classify(spec, group) {
                Group::User => {}
                Group::Ours => ours.push(index),
                Group::Mixed(marked) => {
                    // A hook of neohar beside a hook of the user: take ours out, and add a group.
                    if let Some(array) = group.get("hooks").map(|m| &m.value) {
                        return Ok(Some(remove(array, marked[0])));
                    }
                }
            }
        }
        let wanted = group_text(spec, event, bin, &unit);
        match ours.as_slice() {
            [] => return Ok(Some(push(doc, &member.value, &wanted, &unit))),
            [only] => {
                let group = &groups[*only];
                let indent = line_indent(doc, group.start);
                let wanted = reindent(&wanted, indent);
                if group.text(doc) != wanted {
                    return Ok(Some(replace(group, &wanted)));
                }
            }
            [.., last] => return Ok(Some(remove(&member.value, *last))),
        }
    }
    Ok(None)
}

/// Puts the hooks of neohar in the settings. `text` is the file, or `None` when it does not exist.
/// The result is the new text of the file.
pub fn install(spec: &Spec, text: Option<&str>, bin: &str) -> Result<String> {
    let mut doc = text.map_or_else(|| "{}\n".to_owned(), str::to_owned);
    // Each step is one edit, and the text is read again after it. The events bound the steps.
    for _ in 0..200 {
        let root = root_object(&doc)?;
        match install_step(spec, &doc, &root, bin)? {
            Some(edit) => doc = edit.apply(&doc),
            None => return Ok(doc),
        }
    }
    Err(Error::Format("the installer did not settle".to_owned()))
}

/// The next edit that takes the hooks of neohar out.
fn uninstall_step(spec: &Spec, root: &Node) -> Option<Edit> {
    let hooks = root.get("hooks")?;
    let events = hooks.value.members()?;
    let only_event = events.len() == 1;
    for (event_index, event) in events.iter().enumerate() {
        let Some(groups) = event.value.elements() else {
            continue;
        };
        for (index, group) in groups.iter().enumerate() {
            match classify(spec, group) {
                Group::User => {}
                Group::Ours => {
                    return Some(if groups.len() > 1 {
                        remove(&event.value, index)
                    } else if only_event {
                        // The last hook of the file: the key "hooks" goes with it.
                        let at = root.members()?.iter().position(|m| m.key == "hooks")?;
                        remove(root, at)
                    } else {
                        remove(&hooks.value, event_index)
                    });
                }
                Group::Mixed(marked) => {
                    let array = &group.get("hooks")?.value;
                    return Some(remove(array, marked[0]));
                }
            }
        }
    }
    None
}

/// Takes the hooks of neohar out of the settings and leaves the rest as it was.
pub fn uninstall(spec: &Spec, text: &str) -> Result<String> {
    let mut doc = text.to_owned();
    for _ in 0..200 {
        let root = root_object(&doc)?;
        match uninstall_step(spec, &root) {
            Some(edit) => doc = edit.apply(&doc),
            None => return Ok(doc),
        }
    }
    Err(Error::Format("the installer did not settle".to_owned()))
}

/// How an event stands in a file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Presence {
    /// The hook is there as this version writes it.
    Current,
    /// A hook of neohar is there, and it is not what this version writes.
    Stale,
    Missing,
}

/// The state of one event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EventState {
    pub name: &'static str,
    pub presence: Presence,
    /// The place of the hook of neohar: the index of its group in the array of the event, and of
    /// the hook in the group. Codex keeps the trust of a hook under these numbers.
    pub place: Option<(usize, usize)>,
}

/// Which events have the hook of neohar. `text` is `None` for a file that is not there.
pub fn inspect(spec: &Spec, text: Option<&str>, bin: &str) -> Result<Vec<EventState>> {
    let Some(text) = text else {
        return Ok(spec
            .events
            .iter()
            .map(|e| EventState {
                name: e.name,
                presence: Presence::Missing,
                place: None,
            })
            .collect());
    };
    let root = root_object(text)?;
    let unit = indent_unit(text, &root);
    let hooks = root.get("hooks").map(|m| &m.value);
    let mut out = Vec::new();
    for event in spec.events {
        let groups = hooks
            .and_then(|h| h.get(event.name))
            .and_then(|m| m.value.elements());
        let mut state = EventState {
            name: event.name,
            presence: Presence::Missing,
            place: None,
        };
        for (index, group) in groups.unwrap_or_default().iter().enumerate() {
            let (presence, place) = match classify(spec, group) {
                Group::User => continue,
                Group::Ours => {
                    let indent = line_indent(text, group.start);
                    let wanted = reindent(&group_text(spec, event, bin, &unit), indent);
                    let presence = if group.text(text) == wanted {
                        Presence::Current
                    } else {
                        Presence::Stale
                    };
                    (presence, (index, 0))
                }
                Group::Mixed(marked) => (Presence::Stale, (index, marked[0])),
            };
            // A second hook of neohar for the event is one that the install takes out.
            state.presence = if state.place.is_some() {
                Presence::Stale
            } else {
                presence
            };
            state.place = Some(place);
        }
        out.push(state);
    }
    Ok(out)
}
