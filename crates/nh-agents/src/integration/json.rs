//! The places in a JSON text, and edits at those places.
//!
//! [`parse`] reads a text into a tree whose nodes know where they start and end in the text. An
//! [`Edit`] replaces a range of the text. The functions here make the edits that an installer
//! needs, in the style of the text around them: they add a member after the last one on a line of
//! its own with the same indentation, or on the same line for a text in one line.

use std::ops::Range;

use super::{Error, Result};

/// A value with its place in the text.
#[derive(Clone, Debug)]
pub struct Node {
    pub start: usize,
    pub end: usize,
    pub kind: Kind,
}

#[derive(Clone, Debug)]
pub enum Kind {
    Object(Vec<Member>),
    Array(Vec<Node>),
    String(String),
    /// A number, `true`, `false` or `null`.
    Other,
}

#[derive(Clone, Debug)]
pub struct Member {
    pub key: String,
    /// Where the quote of the key is.
    pub start: usize,
    pub value: Node,
}

impl Node {
    pub fn members(&self) -> Option<&[Member]> {
        match &self.kind {
            Kind::Object(members) => Some(members),
            _ => None,
        }
    }

    pub fn elements(&self) -> Option<&[Node]> {
        match &self.kind {
            Kind::Array(elements) => Some(elements),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match &self.kind {
            Kind::String(text) => Some(text),
            _ => None,
        }
    }

    /// The member `key` of an object.
    pub fn get(&self, key: &str) -> Option<&Member> {
        self.members()?.iter().find(|member| member.key == key)
    }

    pub fn text<'a>(&self, source: &'a str) -> &'a str {
        &source[self.start..self.end]
    }
}

struct Parser<'a> {
    text: &'a str,
    at: usize,
}

fn fail<T>(message: impl Into<String>) -> Result<T> {
    Err(Error::Format(message.into()))
}

impl Parser<'_> {
    fn bytes(&self) -> &[u8] {
        self.text.as_bytes()
    }

    fn skip(&mut self) {
        while self
            .bytes()
            .get(self.at)
            .is_some_and(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n'))
        {
            self.at += 1;
        }
    }

    fn value(&mut self) -> Result<Node> {
        self.skip();
        let start = self.at;
        let kind = match self.bytes().get(self.at) {
            Some(b'{') => self.object()?,
            Some(b'[') => self.array()?,
            Some(b'"') => Kind::String(self.string()?),
            Some(_) => {
                while self.bytes().get(self.at).is_some_and(|b| {
                    !matches!(b, b',' | b']' | b'}' | b' ' | b'\t' | b'\r' | b'\n')
                }) {
                    self.at += 1;
                }
                let token = &self.text[start..self.at];
                if serde_json::from_str::<serde_json::Value>(token).is_err() {
                    return fail(format!("{token:?} is not JSON at byte {start}"));
                }
                Kind::Other
            }
            None => return fail("the text ends where a value should be"),
        };
        Ok(Node {
            start,
            end: self.at,
            kind,
        })
    }

    fn string(&mut self) -> Result<String> {
        let start = self.at;
        self.at += 1;
        loop {
            match self.bytes().get(self.at) {
                Some(b'\\') => self.at += 2,
                Some(b'"') => {
                    self.at += 1;
                    break;
                }
                Some(_) => self.at += 1,
                None => return fail("a string does not end"),
            }
        }
        serde_json::from_str(&self.text[start..self.at])
            .map_err(|e| Error::Format(format!("a string at byte {start} is not JSON: {e}")))
    }

    fn object(&mut self) -> Result<Kind> {
        self.at += 1;
        let mut members = Vec::new();
        self.skip();
        if self.bytes().get(self.at) == Some(&b'}') {
            self.at += 1;
            return Ok(Kind::Object(members));
        }
        loop {
            self.skip();
            if self.bytes().get(self.at) != Some(&b'"') {
                return fail(format!("a key was expected at byte {}", self.at));
            }
            let start = self.at;
            let key = self.string()?;
            self.skip();
            if self.bytes().get(self.at) != Some(&b':') {
                return fail(format!("a colon was expected at byte {}", self.at));
            }
            self.at += 1;
            let value = self.value()?;
            members.push(Member { key, start, value });
            self.skip();
            match self.bytes().get(self.at) {
                Some(b',') => self.at += 1,
                Some(b'}') => {
                    self.at += 1;
                    return Ok(Kind::Object(members));
                }
                _ => {
                    return fail(format!(
                        "a comma or a brace was expected at byte {}",
                        self.at
                    ));
                }
            }
        }
    }

    fn array(&mut self) -> Result<Kind> {
        self.at += 1;
        let mut elements = Vec::new();
        self.skip();
        if self.bytes().get(self.at) == Some(&b']') {
            self.at += 1;
            return Ok(Kind::Array(elements));
        }
        loop {
            elements.push(self.value()?);
            self.skip();
            match self.bytes().get(self.at) {
                Some(b',') => self.at += 1,
                Some(b']') => {
                    self.at += 1;
                    return Ok(Kind::Array(elements));
                }
                _ => {
                    return fail(format!(
                        "a comma or a bracket was expected at byte {}",
                        self.at
                    ));
                }
            }
        }
    }
}

/// Reads a JSON text. Comments are not JSON, so a file with comments is refused.
pub fn parse(text: &str) -> Result<Node> {
    let skip_bom = if text.starts_with('\u{feff}') { 3 } else { 0 };
    let mut parser = Parser { text, at: skip_bom };
    let root = parser.value()?;
    parser.skip();
    if parser.at != text.len() {
        return fail(format!(
            "there is text after the value at byte {}",
            parser.at
        ));
    }
    Ok(root)
}

/// A change of the text: the range is replaced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub range: Range<usize>,
    pub text: String,
}

impl Edit {
    pub fn apply(&self, source: &str) -> String {
        let mut out = String::with_capacity(source.len() + self.text.len());
        out.push_str(&source[..self.range.start]);
        out.push_str(&self.text);
        out.push_str(&source[self.range.end..]);
        out
    }
}

/// The white space at the start of the line that holds `at`.
pub fn line_indent(source: &str, at: usize) -> &str {
    let start = source[..at].rfind('\n').map_or(0, |n| n + 1);
    let line = &source[start..];
    let len = line
        .bytes()
        .take_while(|b| matches!(b, b' ' | b'\t'))
        .count();
    &line[..len]
}

/// The unit of indentation of a text: what a member of the root object adds to the indentation of
/// the root. Two spaces when the text does not say.
pub fn indent_unit(source: &str, root: &Node) -> String {
    if let Some(first) = root.members().and_then(|m| m.first())
        && source[root.start..first.start].contains('\n')
    {
        let inner = line_indent(source, first.start);
        let outer = line_indent(source, root.start);
        if let Some(unit) = inner.strip_prefix(outer)
            && !unit.is_empty()
        {
            return unit.to_owned();
        }
    }
    "  ".to_owned()
}

/// The spans of the children of a container, and the places of its braces.
fn spans(container: &Node) -> Vec<Range<usize>> {
    match &container.kind {
        Kind::Object(members) => members
            .iter()
            .map(|member| member.start..member.value.end)
            .collect(),
        Kind::Array(elements) => elements.iter().map(|e| e.start..e.end).collect(),
        _ => Vec::new(),
    }
}

/// Puts `lines` (a text whose first line is not indented, and whose other lines are indented from
/// zero) at the indentation `indent`.
pub fn reindent(lines: &str, indent: &str) -> String {
    lines.replace('\n', &format!("\n{indent}"))
}

/// Adds a child at the end of an object or an array. `child` is the text of the member
/// (`"key": value`) or of the element, indented from zero.
pub fn push(source: &str, container: &Node, child: &str, unit: &str) -> Edit {
    let children = spans(container);
    let closing = line_indent(source, container.start);
    match children.last() {
        None => {
            let inner = format!("{closing}{unit}");
            Edit {
                range: container.start + 1..container.end - 1,
                text: format!("\n{inner}{}\n{closing}", reindent(child, &inner)),
            }
        }
        Some(last) => {
            let multi_line = source[container.start..children[0].start].contains('\n');
            if multi_line {
                let indent = line_indent(source, children[0].start);
                Edit {
                    range: last.end..last.end,
                    text: format!(",\n{indent}{}", reindent(child, indent)),
                }
            } else {
                Edit {
                    range: last.end..last.end,
                    text: format!(", {child}"),
                }
            }
        }
    }
}

/// Removes the child `index` of an object or an array, with the comma that belongs to it.
pub fn remove(container: &Node, index: usize) -> Edit {
    let children = spans(container);
    let range = if children.len() == 1 {
        container.start + 1..container.end - 1
    } else if index + 1 == children.len() {
        children[index - 1].end..children[index].end
    } else {
        children[index].start..children[index + 1].start
    };
    Edit {
        range,
        text: String::new(),
    }
}

/// Replaces the text of a value.
pub fn replace(node: &Node, text: &str) -> Edit {
    Edit {
        range: node.start..node.end,
        text: text.to_owned(),
    }
}

#[cfg(test)]
mod tests;
