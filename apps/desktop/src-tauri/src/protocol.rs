//! The `nh://` scheme: `nh://blob/<hash>` gives a blob of the store to the page, and
//! `nh://font/` gives the faces of the fonts that the person installed. Every other
//! address is a 404.

use tauri::{
    UriSchemeContext, UriSchemeResponder, Wry,
    http::{Request, Response, StatusCode, header},
};

use crate::fonts::{self, FontRequest};
use crate::policy::blob_name;

/// A blob is named by its hash, so its content never changes.
const BLOB_CACHE: &str = "public, max-age=31536000, immutable";
/// The person can install a font while the app runs, so a window asks again when it loads.
const FONT_CACHE: &str = "no-cache";

fn found(bytes: Vec<u8>, mime: &str, cache: &str) -> Response<Vec<u8>> {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime)
        .header(header::CACHE_CONTROL, cache)
        .header("X-Content-Type-Options", "nosniff")
        .body(bytes)
        .unwrap_or_else(|_| missing())
}

/// The policy of a blob: a document from the store runs in a sandbox with no script and no
/// network, whatever type it has. Tauri treats `nh:` as a local origin of the app.
const BLOB_CSP: &str =
    "sandbox; default-src 'none'; img-src 'self' data:; style-src 'unsafe-inline'";

/// A blob of the store. The store keeps the type of the file, which can be `text/html` or
/// `image/svg+xml`. The page shows blobs only as images, so a blob of another type is an
/// attachment, and every blob has [`BLOB_CSP`].
fn found_blob(bytes: Vec<u8>, mime: &str) -> Response<Vec<u8>> {
    let mut response = found(bytes, mime, BLOB_CACHE);
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        header::HeaderValue::from_static(BLOB_CSP),
    );
    if !is_raster_image(mime) {
        headers.insert(
            header::CONTENT_DISPOSITION,
            header::HeaderValue::from_static("attachment"),
        );
    }
    response
}

/// Whether a type is an image with no script: not SVG, which is a document.
fn is_raster_image(mime: &str) -> bool {
    let base = mime
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    base.strip_prefix("image/")
        .is_some_and(|kind| !kind.is_empty() && !kind.starts_with("svg"))
}

fn missing() -> Response<Vec<u8>> {
    let mut response = Response::new(Vec::new());
    *response.status_mut() = StatusCode::NOT_FOUND;
    response
}

/// Answers a request for an `nh://` address. The backend gives a blob: the core reads it on its own
/// runtime, or the link client gets it from the cache or with `BlobGet` and gives only images.
/// A font is the faces of this Mac in both modes. A font
/// request runs on a thread of the pool, because CoreText and the file read must not hold the
/// thread of the webview.
pub fn handle(
    context: UriSchemeContext<'_, Wry>,
    request: Request<Vec<u8>>,
    responder: UriSchemeResponder,
) {
    let app = context.app_handle().clone();
    // The blob store is the one of the machine of the window that asked.
    let label = context.webview_label().to_owned();
    let uri = request.uri().to_string();
    if let Some(font) = fonts::request(&uri) {
        tauri::async_runtime::spawn_blocking(move || {
            let response = match font {
                FontRequest::Faces { families } => found(
                    fonts::stylesheet(&families).into_bytes(),
                    "text/css; charset=utf-8",
                    FONT_CACHE,
                ),
                FontRequest::File {
                    family,
                    bold,
                    italic,
                } => match fonts::file(&family, bold, italic) {
                    Some((bytes, mime)) => found(bytes, mime, FONT_CACHE),
                    None => missing(),
                },
            };
            responder.respond(response);
        });
        return;
    }
    tauri::async_runtime::spawn(async move {
        let backend = crate::machines::of(&app).and_then(|m| m.backend_of_label(&label).ok());
        let response = match (blob_name(&uri), backend) {
            (Some(name), Some(backend)) => match backend.blob(&name).await {
                Ok(Some((bytes, mime))) => found_blob(bytes, &mime),
                Ok(None) => missing(),
                Err(error) => {
                    tracing::warn!(%error, "cannot read a blob");
                    missing()
                }
            },
            _ => missing(),
        };
        responder.respond(response);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blob_is_a_sandboxed_image_or_an_attachment() {
        let png = found_blob(vec![1], "image/png");
        assert_eq!(png.headers()[header::CONTENT_SECURITY_POLICY], BLOB_CSP);
        assert!(png.headers().get(header::CONTENT_DISPOSITION).is_none());
        for document in [
            "text/html",
            "image/svg+xml",
            "IMAGE/SVG+XML; charset=utf-8",
            "application/pdf",
            "image/",
        ] {
            let blob = found_blob(vec![1], document);
            assert_eq!(
                blob.headers()[header::CONTENT_SECURITY_POLICY],
                BLOB_CSP,
                "{document}"
            );
            assert_eq!(
                blob.headers()[header::CONTENT_DISPOSITION],
                "attachment",
                "{document}"
            );
        }
        assert!(BLOB_CSP.starts_with("sandbox;"));
        assert!(is_raster_image("Image/JPEG"));
    }
}
