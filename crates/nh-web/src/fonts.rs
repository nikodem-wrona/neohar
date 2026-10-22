//! The faces of the fonts that the person installed on the machine of the core.
//!
//! A page matches a family name only among the fonts of its own system. The app serves the files
//! of such families over `nh://font/`, and the listener serves them to a phone over `/font/`. Both
//! use this code: the family list of `ui.font_family`, one `@font-face` rule for each face, and the
//! file of one face. The faces come from a [`FontSource`]. The app gives the CoreText code, and
//! `nh daemon` gives none, because `nh` is also the hook shim and links no framework of the app.
//!
//! A file goes out only when the source names it for the family, so a query cannot read any
//! other file.

use std::path::{Path, PathBuf};

/// One face of a family that a page cannot match by name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontFace {
    pub family: String,
    pub bold: bool,
    pub italic: bool,
    /// The font file.
    pub path: PathBuf,
}

/// Where the faces of a family come from.
pub trait FontSource: Send + Sync + 'static {
    /// The faces of `family` that a page cannot match by name: at most one of each of regular,
    /// bold, italic and bold italic.
    fn faces(&self, family: &str) -> Vec<FontFace>;
}

/// The generic families of CSS: a page has them on its own.
const GENERIC: &[&str] = &[
    "serif",
    "sans-serif",
    "monospace",
    "cursive",
    "fantasy",
    "system-ui",
    "ui-serif",
    "ui-sans-serif",
    "ui-monospace",
    "ui-rounded",
    "math",
    "emoji",
    "fangsong",
    "inherit",
    "initial",
    "unset",
    "revert",
];

/// The most families of one list that get rules.
const MAX_FAMILIES: usize = 16;

/// The families of a CSS `font-family` list in order, without quotes, without the generic ones
/// and without repeats.
pub fn families(list: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for item in split_list(list) {
        let item = item.trim();
        let name = item
            .strip_prefix('"')
            .and_then(|rest| rest.strip_suffix('"'))
            .or_else(|| {
                item.strip_prefix('\'')
                    .and_then(|rest| rest.strip_suffix('\''))
            })
            .unwrap_or(item)
            .trim();
        if name.is_empty()
            || GENERIC.contains(&name.to_ascii_lowercase().as_str())
            || found.iter().any(|known| known == name)
        {
            continue;
        }
        found.push(name.to_owned());
    }
    found.truncate(MAX_FAMILIES);
    found
}

/// The items of a comma list, where a comma between quotes does not split.
fn split_list(list: &str) -> Vec<&str> {
    let mut items = Vec::new();
    let mut quote: Option<char> = None;
    let mut start = 0;
    for (index, ch) in list.char_indices() {
        match (quote, ch) {
            (None, '"' | '\'') => quote = Some(ch),
            (Some(open), close) if open == close => quote = None,
            (None, ',') => {
                items.push(&list[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    items.push(&list[start..]);
    items
}

/// The stylesheet that declares the faces of the families of `list` that the source has: one
/// rule per face. `file` is the address of the file route, for example `nh://font/file` in the
/// app or `/font/file` on the phone.
pub fn stylesheet(source: &dyn FontSource, list: &str, file: &str) -> String {
    families(list)
        .iter()
        .flat_map(|family| source.faces(family))
        .map(|face| rule(&face, file))
        .collect()
}

/// The file of a face and its media type, when the source names a face of that kind for the
/// family.
pub fn file(
    source: &dyn FontSource,
    family: &str,
    bold: bool,
    italic: bool,
) -> Option<(Vec<u8>, &'static str)> {
    let face = source
        .faces(family)
        .into_iter()
        .find(|face| face.bold == bold && face.italic == italic)?;
    let bytes = std::fs::read(&face.path).ok()?;
    Some((bytes, media_type(&face.path)))
}

/// The `@font-face` rule of a face, with its file at `file?family=…&weight=…&style=…`.
pub fn rule(face: &FontFace, file: &str) -> String {
    let weight = if face.bold { "700" } else { "400" };
    let style = if face.italic { "italic" } else { "normal" };
    let url = format!(
        "{file}?family={}&weight={weight}&style={style}",
        form_encode(&face.family)
    );
    format!(
        "@font-face {{ font-family: \"{}\"; font-weight: {weight}; font-style: {style}; src: url(\"{url}\"); }}\n",
        css_string(&face.family)
    )
}

/// A value of a query in the form encoding of HTML: a space is `+`, and each byte that is not a
/// letter, a digit or one of `*-._` is `%XX`.
fn form_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'*' | b'-' | b'.' | b'_' => {
                out.push(char::from(byte));
            }
            b' ' => out.push('+'),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// The text of a CSS string between double quotes.
fn css_string(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

/// The media type of a font file, from its extension.
pub fn media_type(path: &Path) -> &'static str {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase);
    match extension.as_deref() {
        Some("ttf") => "font/ttf",
        Some("otf") => "font/otf",
        Some("ttc") => "font/collection",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}

/// A font request of the listener: the query of `/font/faces.css` or `/font/file`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Request {
    Faces {
        families: String,
    },
    File {
        family: String,
        bold: bool,
        italic: bool,
    },
}

/// The request of a path and a query of the listener, or `None` for a bad one.
pub(crate) fn request(path: &str, query: Option<&str>) -> Option<Request> {
    let mut families = None;
    let mut family = None;
    let mut weight = None;
    let mut style = None;
    for pair in query.unwrap_or("").split('&') {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        let value = form_decode(value)?;
        match key {
            "families" => families = Some(value),
            "family" => family = Some(value),
            "weight" => weight = Some(value),
            "style" => style = Some(value),
            _ => {}
        }
    }
    match path {
        "/font/faces.css" => Some(Request::Faces {
            families: families?,
        }),
        "/font/file" => {
            let bold = match weight.as_deref()? {
                "400" => false,
                "700" => true,
                _ => return None,
            };
            let italic = match style.as_deref()? {
                "normal" => false,
                "italic" => true,
                _ => return None,
            };
            Some(Request::File {
                family: family?,
                bold,
                italic,
            })
        }
        _ => None,
    }
}

/// Decodes a value of a query: `+` is a space and `%XX` is a byte. `None` for a cut escape or
/// bytes that are not UTF-8.
pub(crate) fn form_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' => {
                let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
                out.push(u8::from_str_radix(hex, 16).ok()?);
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            other => {
                out.push(other);
                i += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A source with one family of two faces.
    struct Fake(PathBuf);

    impl FontSource for Fake {
        fn faces(&self, family: &str) -> Vec<FontFace> {
            if family != "Hack Nerd Font Mono" {
                return Vec::new();
            }
            [(false, false), (true, false)]
                .into_iter()
                .map(|(bold, italic)| FontFace {
                    family: family.to_owned(),
                    bold,
                    italic,
                    path: self.0.clone(),
                })
                .collect()
        }
    }

    #[test]
    fn a_family_list_gives_its_named_families_once() {
        assert_eq!(
            families("\"Hack Nerd Font Mono\", Menlo, 'SF Mono', monospace, Menlo"),
            ["Hack Nerd Font Mono", "Menlo", "SF Mono"]
        );
        assert_eq!(families("\"A, B\", C"), ["A, B", "C"]);
        assert!(families("monospace, ui-monospace").is_empty());
    }

    #[test]
    fn a_rule_names_the_face_and_its_file() {
        let face = FontFace {
            family: "Hack Nerd Font Mono".to_owned(),
            bold: true,
            italic: false,
            path: PathBuf::from("/x.ttf"),
        };
        assert_eq!(
            rule(&face, "/font/file"),
            "@font-face { font-family: \"Hack Nerd Font Mono\"; font-weight: 700; font-style: normal; src: url(\"/font/file?family=Hack+Nerd+Font+Mono&weight=700&style=normal\"); }\n"
        );
        assert_eq!(form_encode("a\"b/é"), "a%22b%2F%C3%A9");
    }

    #[test]
    fn the_stylesheet_and_the_file_come_only_from_the_source() {
        let dir = std::env::temp_dir().join(format!("nh-web-fonts-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a directory");
        let path = dir.join("hack.ttf");
        std::fs::write(&path, b"font bytes").expect("a file");
        let source = Fake(path);
        let css = stylesheet(&source, "\"Hack Nerd Font Mono\", Menlo", "/font/file");
        assert_eq!(css.matches("@font-face").count(), 2, "{css}");
        assert_eq!(
            file(&source, "Hack Nerd Font Mono", true, false),
            Some((b"font bytes".to_vec(), "font/ttf"))
        );
        assert_eq!(file(&source, "Hack Nerd Font Mono", false, true), None);
        assert_eq!(file(&source, "Menlo", false, false), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_routes_parse_their_queries() {
        assert_eq!(
            request(
                "/font/faces.css",
                Some("families=%22Hack+Nerd+Font+Mono%22%2C+Menlo")
            ),
            Some(Request::Faces {
                families: "\"Hack Nerd Font Mono\", Menlo".to_owned()
            })
        );
        assert_eq!(
            request(
                "/font/file",
                Some("family=Hack+Nerd+Font+Mono&weight=700&style=italic")
            ),
            Some(Request::File {
                family: "Hack Nerd Font Mono".to_owned(),
                bold: true,
                italic: true
            })
        );
        for (path, query) in [
            ("/font/file", "family=Hack&weight=500&style=normal"),
            ("/font/file", "family=Hack&weight=400&style=oblique"),
            ("/font/file", "weight=400&style=normal"),
            ("/font/faces.css", ""),
            ("/font/other", "families=Hack"),
            ("/font/file", "family=%E2%28&weight=400&style=normal"),
        ] {
            assert_eq!(request(path, Some(query)), None, "{path}?{query}");
        }
    }
}
