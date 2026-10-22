//! An independent reader for the HTML that `nh-rich` writes, and the rules it must obey.
//!
//! The sanitizer serializes with one format: tags in lower case, attributes in double quotes with
//! `&amp;`, `&quot;`, `&lt;` and `&gt;` escaped, and every `<` in text escaped. This reader
//! relies on that and panics on anything else, so a change in the format shows at once.

#![allow(dead_code)]

#[derive(Debug)]
pub struct Tag {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub closing: bool,
}

fn decode(value: &str) -> String {
    value
        .replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

/// Every tag of the HTML, in order. Panics when the HTML is not in the format of the serializer.
pub fn tags(html: &str) -> Vec<Tag> {
    let bytes = html.as_bytes();
    let mut out = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] != b'<' {
            at += 1;
            continue;
        }
        at += 1;
        let closing = bytes.get(at) == Some(&b'/');
        if closing {
            at += 1;
        }
        let name_start = at;
        while at < bytes.len() && bytes[at].is_ascii_alphanumeric() {
            at += 1;
        }
        assert!(
            at > name_start,
            "a '<' that is not a tag, near {:?}",
            &html[name_start.saturating_sub(10)..(name_start + 20).min(html.len())]
        );
        let name = html[name_start..at].to_owned();
        assert_eq!(
            name,
            name.to_ascii_lowercase(),
            "tag name in upper case: {name}"
        );
        let mut attrs = Vec::new();
        loop {
            while bytes.get(at) == Some(&b' ') {
                at += 1;
            }
            match bytes.get(at) {
                Some(b'>') => {
                    at += 1;
                    break;
                }
                Some(b'/') if bytes.get(at + 1) == Some(&b'>') => {
                    at += 2;
                    break;
                }
                Some(_) => {
                    let attr_start = at;
                    while at < bytes.len() && !matches!(bytes[at], b'=' | b' ' | b'>' | b'/') {
                        at += 1;
                    }
                    let attr = html[attr_start..at].to_owned();
                    assert!(!attr.is_empty(), "empty attribute name in {name}");
                    if bytes.get(at) == Some(&b'=') {
                        at += 1;
                        assert_eq!(
                            bytes.get(at),
                            Some(&b'"'),
                            "attribute {attr} of {name} has no quotes"
                        );
                        at += 1;
                        let value_start = at;
                        while at < bytes.len() && bytes[at] != b'"' {
                            assert_ne!(bytes[at], b'<', "a raw '<' in attribute {attr}");
                            at += 1;
                        }
                        assert!(at < bytes.len(), "attribute {attr} is not closed");
                        attrs.push((attr, decode(&html[value_start..at])));
                        at += 1;
                    } else {
                        attrs.push((attr, String::new()));
                    }
                }
                None => panic!("tag {name} is not closed"),
            }
        }
        out.push(Tag {
            name,
            attrs,
            closing,
        });
    }
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

const CLASSES: &[&str] = &[
    "kw", "str", "num", "com", "fn", "ty", "const", "op", "tag", "attr", "var", "esc", "err",
    "cmd", "opt",
];

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%' {
            let hex = std::str::from_utf8(&bytes[at + 1..at + 3]).expect("hex");
            out.push(u8::from_str_radix(hex, 16).expect("hex digits"));
            at += 3;
        } else {
            out.push(bytes[at]);
            at += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Asserts that the HTML has only the allowed tags, attributes and links.
pub fn assert_safe(html: &str) {
    for tag in tags(html) {
        assert!(
            TAGS.contains(&tag.name.as_str()),
            "forbidden tag <{}> in {html}",
            tag.name
        );
        if tag.closing {
            assert!(tag.attrs.is_empty(), "a closing tag with attributes");
            continue;
        }
        for (name, value) in &tag.attrs {
            assert!(
                !name.starts_with("on"),
                "event handler {name} on <{}>",
                tag.name
            );
            let allowed: &[&str] = match tag.name.as_str() {
                "a" => &["href", "title", "rel"],
                "img" => &["src", "alt", "title"],
                "code" | "pre" | "span" => &["class"],
                "td" | "th" => &["style"],
                "ol" => &["start"],
                _ => &[],
            };
            assert!(
                allowed.contains(&name.as_str()),
                "attribute {name} on <{}>: {value}",
                tag.name
            );
            match (tag.name.as_str(), name.as_str()) {
                ("a", "href") => {
                    let url = value
                        .strip_prefix("nh://open?url=")
                        .unwrap_or_else(|| panic!("href that is not nh://open: {value}"));
                    assert!(
                        !url.contains("://") && !url.contains(' '),
                        "url not encoded: {url}"
                    );
                    let decoded = percent_decode(url);
                    let scheme = decoded.split_once(':').map(|(s, _)| s.to_ascii_lowercase());
                    assert!(
                        matches!(scheme.as_deref(), Some("http" | "https" | "mailto")),
                        "scheme of {decoded}"
                    );
                }
                ("a", "rel") => assert_eq!(value, "noopener noreferrer"),
                ("img", "src") => {
                    let hash = value
                        .strip_prefix("nh://blob/")
                        .unwrap_or_else(|| panic!("src that is not a blob: {value}"));
                    assert!(
                        !hash.is_empty()
                            && hash
                                .bytes()
                                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
                        "blob name {hash}"
                    );
                }
                (_, "class") => {
                    assert!(!value.is_empty());
                    for class in value.split(' ') {
                        let ok = match tag.name.as_str() {
                            "span" => class
                                .strip_prefix("hl-")
                                .is_some_and(|c| CLASSES.contains(&c)),
                            _ => class.strip_prefix("language-").is_some_and(|c| {
                                !c.is_empty()
                                    && c.bytes().all(|b| {
                                        b.is_ascii_lowercase()
                                            || b.is_ascii_digit()
                                            || matches!(b, b'_' | b'+' | b'#' | b'-')
                                    })
                            }),
                        };
                        assert!(ok, "class {class} on <{}>", tag.name);
                    }
                }
                (_, "style") => assert!(
                    matches!(
                        value.as_str(),
                        "text-align: left" | "text-align: right" | "text-align: center"
                    ),
                    "style {value}"
                ),
                ("ol", "start") => {
                    assert!(value.bytes().all(|b| b.is_ascii_digit()), "start {value}")
                }
                _ => {}
            }
        }
    }
}
