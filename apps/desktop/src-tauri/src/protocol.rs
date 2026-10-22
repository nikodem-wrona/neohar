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
                Ok(Some((bytes, mime))) => found(bytes, &mime, BLOB_CACHE),
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
