//! The faces of the fonts that the person installed.
//!
//! WebKit on macOS matches a family name only among the fonts of the system. A font that the
//! person installed, in `~/Library/Fonts` or `/Library/Fonts`, is found for no family name,
//! alone or in a list: the page gets the next family of the list, or the standard font. The app
//! finds the files of such families with CoreText and serves them over `nh://font/`, and the
//! page declares them with `@font-face`. Then the list of `ui.font_family` works as the person
//! wrote it, and a Nerd Font shows its icons.
//!
//! On Linux the webview sees every font that fontconfig knows, and the stylesheet is empty.

use std::path::{Path, PathBuf};

use tauri::Url;

/// What an `nh://font/` address asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FontRequest {
    /// `nh://font/faces.css?families=<list>`: the stylesheet of the faces of a family list.
    Faces { families: String },
    /// `nh://font/file?family=<name>&weight=<400|700>&style=<normal|italic>`: one font file.
    File {
        family: String,
        bold: bool,
        italic: bool,
    },
}

/// The request of an `nh://font/` address, or `None` for any other address or a bad query.
pub fn request(uri: &str) -> Option<FontRequest> {
    let url = Url::parse(uri).ok()?;
    if url.scheme() != "nh" || url.host_str() != Some("font") {
        return None;
    }
    let mut families = None;
    let mut family = None;
    let mut weight = None;
    let mut style = None;
    for (key, value) in url.query_pairs() {
        match &*key {
            "families" => families = Some(value.into_owned()),
            "family" => family = Some(value.into_owned()),
            "weight" => weight = Some(value.into_owned()),
            "style" => style = Some(value.into_owned()),
            _ => {}
        }
    }
    match url.path() {
        "/faces.css" => Some(FontRequest::Faces {
            families: families?,
        }),
        "/file" => {
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
            Some(FontRequest::File {
                family: family?,
                bold,
                italic,
            })
        }
        _ => None,
    }
}

/// One face of a family that the webview cannot match by name.
pub use nh_web::FontFace as Face;
#[cfg(test)]
use nh_web::fonts::{families, media_type, rule};

/// The address of the file route of the app.
const FILE_ROUTE: &str = "nh://font/file";

/// The fonts of the person, as CoreText finds them. The app gives the same source to the listener
/// of the phone, which serves the faces over `/font/`.
#[derive(Debug, Clone, Copy, Default)]
pub struct CoreTextFonts;

impl nh_web::FontSource for CoreTextFonts {
    fn faces(&self, family: &str) -> Vec<Face> {
        user_faces(family)
    }
}

/// The stylesheet that declares the faces of the families of `list` that the webview cannot
/// match by name: one rule per face, with the file at `nh://font/file`.
pub fn stylesheet(list: &str) -> String {
    nh_web::fonts::stylesheet(&CoreTextFonts, list, FILE_ROUTE)
}

/// The file of a face and its media type, when the family is one that the webview cannot match
/// by name.
pub fn file(family: &str, bold: bool, italic: bool) -> Option<(Vec<u8>, &'static str)> {
    nh_web::fonts::file(&CoreTextFonts, family, bold, italic)
}

/// A face as CoreText reports it, before the choice of one face per kind.
#[derive(Debug, Clone, PartialEq)]
struct Candidate {
    bold: bool,
    italic: bool,
    /// The weight trait of CoreText: 0 for a regular face, 0.4 for a bold one.
    weight: f64,
    path: PathBuf,
}

/// The weight trait of a bold face.
const BOLD_WEIGHT: f64 = 0.4;

/// The faces of `family` that the webview cannot match by name: at most one of each kind.
fn user_faces(family: &str) -> Vec<Face> {
    let candidates: Vec<Candidate> = installed(family)
        .into_iter()
        .filter(|candidate| !is_system(&candidate.path))
        .collect();
    pick(family, &candidates)
}

/// A font under `/System` comes with macOS, and the webview matches it on its own.
fn is_system(path: &Path) -> bool {
    path.starts_with("/System")
}

/// At most four faces: for each of regular, bold, italic and bold italic, the candidate of that
/// kind whose weight is nearest to the weight of a regular or a bold face.
fn pick(family: &str, candidates: &[Candidate]) -> Vec<Face> {
    let mut faces = Vec::new();
    for (bold, italic) in [(false, false), (true, false), (false, true), (true, true)] {
        let target = if bold { BOLD_WEIGHT } else { 0.0 };
        let best = candidates
            .iter()
            .filter(|candidate| candidate.bold == bold && candidate.italic == italic)
            .min_by(|a, b| {
                (a.weight - target)
                    .abs()
                    .total_cmp(&(b.weight - target).abs())
            });
        if let Some(candidate) = best {
            faces.push(Face {
                family: family.to_owned(),
                bold,
                italic,
                path: candidate.path.clone(),
            });
        }
    }
    faces
}

/// The faces of a family as CoreText knows them, with their files.
#[cfg(target_os = "macos")]
fn installed(family: &str) -> Vec<Candidate> {
    use objc2_core_foundation::{CFDictionary, CFNumber, CFString, CFType, CFURL};
    use objc2_core_text::{
        CTFontDescriptor, kCTFontFamilyNameAttribute, kCTFontSymbolicTrait, kCTFontTraitsAttribute,
        kCTFontURLAttribute, kCTFontWeightTrait,
    };

    /// `kCTFontTraitItalic` and `kCTFontTraitBold` of the symbolic traits.
    const ITALIC: i64 = 1 << 0;
    const BOLD: i64 = 1 << 1;

    let name = CFString::from_str(family);
    // SAFETY: the statics are the constant keys of CoreText, and every function gets references
    // that live for the call.
    let found = unsafe {
        let attributes = CFDictionary::from_slices(&[kCTFontFamilyNameAttribute], &[&*name]);
        CTFontDescriptor::with_attributes(attributes.as_opaque()).matching_font_descriptors(None)
    };
    let Some(found) = found else {
        return Vec::new();
    };
    // SAFETY: CoreText answers with an array of font descriptors.
    let descriptors = unsafe { found.cast_unchecked::<CTFontDescriptor>() };
    let mut candidates = Vec::new();
    for descriptor in descriptors.iter() {
        // SAFETY: the keys are constants of CoreText.
        let (url, traits) = unsafe {
            (
                descriptor.attribute(kCTFontURLAttribute),
                descriptor.attribute(kCTFontTraitsAttribute),
            )
        };
        let Some(path) = url
            .and_then(|url| url.downcast::<CFURL>().ok())
            .and_then(|url| url.to_file_path())
        else {
            continue;
        };
        let Some(traits) = traits.and_then(|traits| traits.downcast::<CFDictionary>().ok()) else {
            continue;
        };
        // SAFETY: the traits of a font are numbers under the keys of CoreText.
        let traits = unsafe { traits.cast_unchecked::<CFType, CFType>() };
        let number = |key: &CFString| {
            let key: &CFType = key;
            traits
                .get(key)
                .and_then(|value| value.downcast::<CFNumber>().ok())
        };
        // SAFETY: the keys are constants of CoreText.
        let (symbolic, weight) =
            unsafe { (number(kCTFontSymbolicTrait), number(kCTFontWeightTrait)) };
        let symbolic = symbolic.and_then(|number| number.as_i64()).unwrap_or(0);
        candidates.push(Candidate {
            bold: symbolic & BOLD != 0,
            italic: symbolic & ITALIC != 0,
            weight: weight.and_then(|number| number.as_f64()).unwrap_or(0.0),
            path,
        });
    }
    candidates
}

#[cfg(not(target_os = "macos"))]
fn installed(_family: &str) -> Vec<Candidate> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_addresses_of_the_font_routes_parse() {
        assert_eq!(
            request("nh://font/faces.css?families=%22Hack+Nerd+Font+Mono%22%2C+Menlo"),
            Some(FontRequest::Faces {
                families: "\"Hack Nerd Font Mono\", Menlo".to_owned()
            })
        );
        assert_eq!(
            request("nh://font/file?family=Hack+Nerd+Font+Mono&weight=700&style=italic"),
            Some(FontRequest::File {
                family: "Hack Nerd Font Mono".to_owned(),
                bold: true,
                italic: true,
            })
        );
        for bad in [
            "nh://font/file?family=Hack&weight=500&style=normal",
            "nh://font/file?family=Hack&weight=400&style=oblique",
            "nh://font/file?weight=400&style=normal",
            "nh://font/faces.css",
            "nh://font/other?families=Hack",
            "nh://blob/faces.css?families=Hack",
            "https://font/faces.css?families=Hack",
        ] {
            assert_eq!(request(bad), None, "{bad}");
        }
    }

    #[test]
    fn a_family_list_gives_its_named_families_once() {
        assert_eq!(
            families(
                "\"Hack Nerd Font Mono\", Menlo, 'SF Mono', \"DejaVu Sans Mono\", monospace, Menlo, \"a, b\""
            ),
            [
                "Hack Nerd Font Mono",
                "Menlo",
                "SF Mono",
                "DejaVu Sans Mono",
                "a, b"
            ]
        );
        assert_eq!(families("monospace"), Vec::<String>::new());
        assert_eq!(families(""), Vec::<String>::new());
    }

    #[test]
    fn a_rule_names_the_face_and_its_file() {
        let face = Face {
            family: "Hack Nerd Font Mono".to_owned(),
            bold: true,
            italic: false,
            path: PathBuf::from("/Users/me/Library/Fonts/HackNerdFontMono-Bold.ttf"),
        };
        assert_eq!(
            rule(&face, FILE_ROUTE),
            "@font-face { font-family: \"Hack Nerd Font Mono\"; font-weight: 700; font-style: normal; src: url(\"nh://font/file?family=Hack+Nerd+Font+Mono&weight=700&style=normal\"); }\n"
        );
        // The address of a rule parses back to the face.
        assert_eq!(
            request("nh://font/file?family=Hack+Nerd+Font+Mono&weight=700&style=normal"),
            Some(FontRequest::File {
                family: face.family.clone(),
                bold: true,
                italic: false,
            })
        );
    }

    #[test]
    fn the_choice_takes_the_nearest_weight_of_each_kind() {
        let candidate = |bold, italic, weight: f64, name: &str| Candidate {
            bold,
            italic,
            weight,
            path: PathBuf::from(format!("/Users/me/Library/Fonts/{name}.otf")),
        };
        let candidates = [
            candidate(false, false, -0.4, "Light"),
            candidate(false, false, 0.0, "Regular"),
            candidate(false, false, 0.23, "Medium"),
            candidate(true, false, 0.4, "Bold"),
            candidate(true, false, 0.56, "Heavy"),
            candidate(false, true, 0.0, "Italic"),
        ];
        let faces = pick("Mono", &candidates);
        let names: Vec<_> = faces
            .iter()
            .map(|face| {
                (
                    face.bold,
                    face.italic,
                    face.path.file_stem().and_then(|s| s.to_str()).unwrap_or(""),
                )
            })
            .collect();
        assert_eq!(
            names,
            [
                (false, false, "Regular"),
                (true, false, "Bold"),
                (false, true, "Italic")
            ]
        );
    }

    #[test]
    fn the_media_type_follows_the_extension() {
        assert_eq!(media_type(Path::new("/a/b.TTF")), "font/ttf");
        assert_eq!(media_type(Path::new("/a/b.otf")), "font/otf");
        assert_eq!(media_type(Path::new("/a/b.ttc")), "font/collection");
        assert_eq!(media_type(Path::new("/a/b")), "application/octet-stream");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_font_of_the_system_gets_no_rule_and_an_unknown_family_none() {
        // Menlo comes with macOS, under `/System`, so the webview matches it on its own.
        let menlo = installed("Menlo");
        assert!(!menlo.is_empty(), "CoreText knows Menlo");
        assert!(
            menlo.iter().all(|candidate| is_system(&candidate.path)),
            "{menlo:?}"
        );
        assert_eq!(stylesheet("Menlo, monospace"), "");
        assert_eq!(file("Menlo", false, false), None);
        assert_eq!(stylesheet("\"No Such Family 1234\""), "");
    }
}
