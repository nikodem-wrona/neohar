//! The files of the phone app in the binary. `build.rs` makes the table.

include!(concat!(env!("OUT_DIR"), "/files.rs"));

/// The page when the binary has no phone app: a build without `NEOHAR_WEB_DIST`.
const NOT_BUILT: &[u8] = b"<!doctype html>\n<html lang=\"en\">\n<head><meta charset=\"utf-8\">\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
<title>neohar</title></head>\n<body><p>The phone app is not built into this neohar.</p></body>\n</html>\n";

/// The content type of a path, from its extension.
pub(crate) fn kind_of(path: &str) -> &'static str {
    match path.rsplit_once('.').map(|(_, ext)| ext) {
        Some("html") => "text/html; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json",
        Some("webmanifest") => "application/manifest+json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("ico") => "image/x-icon",
        Some("wasm") => "application/wasm",
        Some("woff2") => "font/woff2",
        Some("txt") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// A file of the app by its path without the first `/`.
pub(crate) fn file(path: &str) -> Option<&'static [u8]> {
    FILES
        .iter()
        .find(|(name, _)| *name == path)
        .map(|(_, bytes)| *bytes)
}

/// The page of the app: `index.html`, or the page that says the app is not built.
pub(crate) fn index() -> &'static [u8] {
    file("index.html").unwrap_or(NOT_BUILT)
}

/// The icons of the app for the manifest: the files `icon-<size>.png`.
pub(crate) fn icons() -> Vec<(&'static str, &'static str)> {
    FILES
        .iter()
        .filter_map(|(name, _)| {
            let size = name.strip_prefix("icon-")?.strip_suffix(".png")?;
            Some((*name, size))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_has_the_type_of_its_extension() {
        assert_eq!(kind_of("index.html"), "text/html; charset=utf-8");
        assert_eq!(
            kind_of("assets/app-1a2b.js"),
            "text/javascript; charset=utf-8"
        );
        assert_eq!(kind_of("icon-192.png"), "image/png");
        assert_eq!(kind_of("README"), "application/octet-stream");
    }

    #[test]
    fn the_page_exists_with_or_without_the_app() {
        assert!(index().starts_with(b"<!doctype html>"));
    }
}
