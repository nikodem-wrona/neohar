//! Markdown as sanitized HTML for the plan view.
//!
//! A plan comes from an agent, and an agent reads files, web pages and tool output that anyone can
//! write. The text is untrusted. `pulldown-cmark` reads the markdown, the code blocks get their
//! highlight, and `ammonia` then cleans the HTML with a list of what is allowed. The list is short:
//!
//! - Tags for text, lists, tables and code. No `script`, `style`, `iframe`, `object`, `embed`,
//!   `form`, `svg` or `math`. The content of a few of these is dropped, and the content of the others
//!   stays as text.
//! - No attribute but `href`, `src`, `alt`, `title`, `class`, `style` and `start`, each on the tags
//!   that need it. No event handler and no `id`. A `style` is allowed on the cells of a table, with
//!   the three values of `text-align` and nothing else.
//! - A link to `http`, `https` or `mailto` becomes `nh://open?url=<percent-encoded>`, and the UI opens
//!   it in the browser of the system. A link with any other scheme, a relative link and an anchor
//!   lose their `href`.
//! - An image has a `src` only when it is `nh://blob/<name>`. Nothing is loaded from the network.
//! - A `class` has only the values that this crate writes: `hl-<class>` on a `span`, and
//!   `language-<name>` on `pre` and `code`. A page cannot use the CSS classes of the UI.
//!
//! A client does not trust the HTML of a core on another machine, because that core can send any
//! text as its HTML. [`clean`] applies the same lists again to HTML that a core made, and
//! [`clean_html_fields`] does it for each field `html` of a value of the API.

use std::{borrow::Cow, collections::HashSet, sync::OnceLock};

use ammonia::{Builder, UrlRelative};
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd, html};

use crate::{
    highlight::{self, class, highlight_html},
    text::{cap, strip_terminal_controls},
};

/// The largest plan that is rendered: 256 KB.
pub const PLAN_CAP: usize = 256 * 1024;

/// Sanitized HTML.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Markdown {
    pub html: String,
    /// The text was cut at [`PLAN_CAP`].
    pub truncated: bool,
}

/// The HTML of a markdown text.
pub fn markdown(text: &str) -> Markdown {
    // An OSC 8 hyperlink in the text has a URL that the reader does not see. It goes away first.
    let clean = strip_terminal_controls(text);
    let capped = cap(&clean, PLAN_CAP);
    let options =
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let mut events: Vec<Event<'_>> = Vec::new();
    // The language and the text of the code block that is being read.
    let mut code: Option<(String, String)> = None;
    for event in Parser::new_ext(capped.text, options) {
        match event {
            Event::Start(Tag::CodeBlock(kind)) => {
                let language = match kind {
                    CodeBlockKind::Fenced(info) => {
                        info.split_whitespace().next().unwrap_or("").to_owned()
                    }
                    CodeBlockKind::Indented => String::new(),
                };
                code = Some((language, String::new()));
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some((language, source)) = code.take() {
                    events.push(Event::Html(code_block(&language, &source).into()));
                }
            }
            Event::Text(text) if code.is_some() => {
                if let Some((_, source)) = code.as_mut() {
                    source.push_str(&text);
                }
            }
            // A check box is a form control. The UI draws a sign instead.
            Event::TaskListMarker(done) => {
                events.push(Event::Text(if done { "☑ " } else { "☐ " }.into()));
            }
            other => events.push(other),
        }
    }
    let mut dirty = String::with_capacity(capped.text.len() * 3 / 2);
    html::push_html(&mut dirty, events.into_iter());
    Markdown {
        html: sanitizer().clean(&dirty).to_string(),
        truncated: capped.truncated,
    }
}

/// A code block as HTML: `<pre><code class="language-x">` with a span for each highlighted range.
fn code_block(language: &str, source: &str) -> String {
    let name: String = language
        .chars()
        .take(32)
        .map(|ch| ch.to_ascii_lowercase())
        .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '+' | '#' | '-'))
        .collect();
    let mut out = String::with_capacity(source.len() * 2);
    out.push_str("<pre><code");
    if !name.is_empty() {
        out.push_str(" class=\"language-");
        out.push_str(&name);
        out.push('"');
    }
    out.push('>');
    out.push_str(&highlight_html(source, highlight::language(language)));
    out.push_str("</code></pre>\n");
    out
}

const TAGS: &[&str] = &[
    "a",
    "abbr",
    "b",
    "blockquote",
    "br",
    "code",
    "dd",
    "del",
    "details",
    "dl",
    "dt",
    "em",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "hr",
    "i",
    "img",
    "kbd",
    "li",
    "mark",
    "ol",
    "p",
    "pre",
    "s",
    "span",
    "strong",
    "sub",
    "summary",
    "sup",
    "table",
    "tbody",
    "td",
    "tfoot",
    "th",
    "thead",
    "tr",
    "ul",
];

/// Tags whose content goes away with the tag. The content of the other tags that are not allowed
/// stays as text.
const DROP_CONTENT: &[&str] = &[
    "script", "style", "iframe", "object", "embed", "applet", "template", "noscript", "svg",
    "math", "title", "textarea", "select", "option", "button", "head",
];

/// The cleaner of the HTML that `pulldown-cmark` makes from a text.
fn sanitizer() -> &'static Builder<'static> {
    static BUILDER: OnceLock<Builder<'static>> = OnceLock::new();
    BUILDER.get_or_init(|| {
        let mut builder = lists();
        builder.attribute_filter(filter_attribute);
        builder
    })
}

/// The cleaner of HTML that a core made: a link is already `nh://open?url=…`.
fn cleaner() -> &'static Builder<'static> {
    static BUILDER: OnceLock<Builder<'static>> = OnceLock::new();
    BUILDER.get_or_init(|| {
        let mut builder = lists();
        builder.attribute_filter(filter_made_attribute);
        builder
    })
}

/// Cleans HTML that a core made with [`markdown`] again, with the same lists. A client calls it
/// for the HTML of a core on another machine, which can send any text. The HTML of [`markdown`]
/// does not change: `clean(&markdown(t).html)` is `markdown(t).html`.
pub fn clean(html: &str) -> String {
    cleaner().clean(html).to_string()
}

/// Cleans the text of each field `html` in a value of the API, at any depth, with [`clean`].
/// Every type of the API that carries HTML has it in a field `html` (`Markdown`,
/// `TimelineItemText`), and the UI inserts only such a field. Returns the number of fields that
/// it cleaned.
pub fn clean_html_fields(value: &mut serde_json::Value) -> usize {
    use serde_json::Value;
    match value {
        Value::Object(map) => map
            .iter_mut()
            .map(|(key, field)| match field {
                Value::String(html) if key == "html" => {
                    *html = clean(html);
                    1
                }
                other => clean_html_fields(other),
            })
            .sum(),
        Value::Array(items) => items.iter_mut().map(clean_html_fields).sum(),
        _ => 0,
    }
}

/// The lists of tags and attributes of both cleaners, without the last gate for each attribute.
fn lists() -> Builder<'static> {
    let mut builder = Builder::empty();
    builder
        .tags(TAGS.iter().copied().collect())
        .clean_content_tags(DROP_CONTENT.iter().copied().collect())
        .tag_attributes(
            [
                ("a", ["href", "title"].into_iter().collect::<HashSet<_>>()),
                ("img", ["src", "alt", "title"].into_iter().collect()),
                ("code", ["class"].into_iter().collect()),
                ("pre", ["class"].into_iter().collect()),
                ("span", ["class"].into_iter().collect()),
                ("td", ["style"].into_iter().collect()),
                ("th", ["style"].into_iter().collect()),
                ("ol", ["start"].into_iter().collect()),
            ]
            .into_iter()
            .collect(),
        )
        // `nh` is for the image sources that pass the filter below. A link with the scheme
        // `nh` does not pass it.
        .url_schemes(["http", "https", "mailto", "nh"].into_iter().collect())
        .url_relative(UrlRelative::Deny)
        .link_rel(Some("noopener noreferrer"))
        .strip_comments(true);
    builder
}

/// The last gate for each attribute. It runs after the lists of ammonia.
fn filter_attribute<'a>(element: &str, attribute: &str, value: &'a str) -> Option<Cow<'a, str>> {
    match (element, attribute) {
        ("a", "href") => open_link(value).map(Cow::Owned),
        ("img", "src") => blob_source(value).then_some(Cow::Borrowed(value)),
        (_, "class") => allowed_classes(element, value).then_some(Cow::Borrowed(value)),
        // The one style: the alignment that `pulldown-cmark` writes for a column of a table.
        ("td" | "th", "style") => matches!(
            value,
            "text-align: left" | "text-align: right" | "text-align: center"
        )
        .then_some(Cow::Borrowed(value)),
        ("ol", "start") => {
            (!value.is_empty() && value.len() <= 9 && value.bytes().all(|b| b.is_ascii_digit()))
                .then_some(Cow::Borrowed(value))
        }
        _ => Some(Cow::Borrowed(value)),
    }
}

/// The last gate for HTML that a core made: a link must be `nh://open?url=…` with an address that
/// [`open_link`] takes. The other attributes have the gate of [`filter_attribute`].
fn filter_made_attribute<'a>(
    element: &str,
    attribute: &str,
    value: &'a str,
) -> Option<Cow<'a, str>> {
    match (element, attribute) {
        ("a", "href") => made_link(value).and_then(|target| open_link(&target).map(Cow::Owned)),
        _ => filter_attribute(element, attribute, value),
    }
}

/// The address in a link that [`open_link`] made: `nh://open?url=` and the percent-encoded
/// address, with nothing after it.
fn made_link(value: &str) -> Option<String> {
    let encoded = value.strip_prefix("nh://open?url=")?;
    let mut bytes = Vec::with_capacity(encoded.len());
    let mut rest = encoded.bytes();
    while let Some(byte) = rest.next() {
        match byte {
            b'%' => {
                let high = char::from(rest.next()?).to_digit(16)?;
                let low = char::from(rest.next()?).to_digit(16)?;
                bytes.push(u8::try_from(high * 16 + low).ok()?);
            }
            b if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') => {
                bytes.push(b);
            }
            _ => return None,
        }
    }
    String::from_utf8(bytes).ok()
}

/// `nh://open?url=…` for a link that goes to the web or to a mail address.
fn open_link(value: &str) -> Option<String> {
    let value = value.trim();
    let (scheme, rest) = value.split_once(':')?;
    let scheme_ok = !scheme.is_empty()
        && scheme
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.'));
    if !scheme_ok {
        return None;
    }
    let scheme = scheme.to_ascii_lowercase();
    let allowed = match scheme.as_str() {
        "http" | "https" => rest.starts_with("//") && rest.len() > 2,
        "mailto" => !rest.is_empty(),
        _ => false,
    };
    if !allowed {
        return None;
    }
    let mut out = String::from("nh://open?url=");
    percent_encode(value, &mut out);
    Some(out)
}

fn percent_encode(text: &str, out: &mut String) {
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
}

/// `nh://blob/<name>`, where the name is a hash: letters, digits, `-` and `_`.
fn blob_source(value: &str) -> bool {
    value.strip_prefix("nh://blob/").is_some_and(|name| {
        !name.is_empty()
            && name.len() <= 128
            && name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
    })
}

/// The classes that this crate writes. Any other class could use the CSS of the UI.
fn allowed_classes(element: &str, value: &str) -> bool {
    let mut any = false;
    for token in value.split_ascii_whitespace() {
        any = true;
        let ok = match element {
            "span" => token
                .strip_prefix("hl-")
                .is_some_and(|name| class::ALL.contains(&name)),
            "code" | "pre" => token.strip_prefix("language-").is_some_and(|name| {
                !name.is_empty()
                    && name.len() <= 32
                    && name.bytes().all(|b| {
                        b.is_ascii_lowercase()
                            || b.is_ascii_digit()
                            || matches!(b, b'_' | b'+' | b'#' | b'-')
                    })
            }),
            _ => false,
        };
        if !ok {
            return false;
        }
    }
    any
}

#[cfg(test)]
mod tests;
