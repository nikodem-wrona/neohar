//! The rules of the host that need no window: where a webview may go, and what an
//! `nh://` address names. They are plain functions, so a test can check them.

use tauri::Url;

/// What to do with a navigation of a webview.
#[derive(Debug, PartialEq, Eq)]
pub enum Navigation {
    /// The page of the app. Let it load.
    Allow,
    /// A link that the page asked to open outside the app: cancel the navigation and open the
    /// address with the system (`nh://open?url=...`).
    OpenOutside(String),
    /// Anything else. Cancel it and write a log line.
    Deny,
}

/// The origin that the dev server of the UI has in a debug build.
const DEV_ORIGIN: (&str, &str, u16) = ("http", "localhost", 1420);

/// Whether `url` is a page of the app: the files of the bundle, or the dev server of a debug
/// build.
fn is_app(url: &Url) -> bool {
    match url.scheme() {
        // The pages of a bundle on macOS and Linux.
        "tauri" => url.host_str() == Some("localhost"),
        // The pages of a bundle on Windows.
        "http" | "https" if url.host_str() == Some("tauri.localhost") => true,
        "http" => {
            cfg!(debug_assertions)
                && url.host_str() == Some(DEV_ORIGIN.1)
                && url.port() == Some(DEV_ORIGIN.2)
        }
        "about" => url.as_str() == "about:blank",
        _ => false,
    }
}

/// Decides what a webview does with a navigation.
///
/// - A page of the app loads.
/// - `nh://open?url=<address>` opens the address outside the app if its scheme is `http`, `https`
///   or `mailto`.
/// - Everything else is denied: a link in rich content must not take the webview away from the
///   app, and the webview must not load a page from the network.
pub fn navigation(url: &Url) -> Navigation {
    if is_app(url) {
        return Navigation::Allow;
    }
    if url.scheme() == "nh" && url.host_str() == Some("open") {
        let target = url
            .query_pairs()
            .find(|(key, _)| key == "url")
            .map(|(_, value)| value.into_owned());
        return match target.as_deref().map(Url::parse) {
            Some(Ok(target)) if matches!(target.scheme(), "http" | "https" | "mailto") => {
                Navigation::OpenOutside(target.into())
            }
            _ => Navigation::Deny,
        };
    }
    Navigation::Deny
}

/// The hash of `nh://blob/<hash>`, or `None` for any other address. The core checks that the name is
/// a hash. This function only finds the name.
pub fn blob_name(uri: &str) -> Option<String> {
    let url = Url::parse(uri).ok()?;
    if url.scheme() != "nh" || url.host_str() != Some("blob") {
        return None;
    }
    let name = url.path().strip_prefix('/')?;
    (!name.is_empty() && !name.contains('/')).then(|| name.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(text: &str) -> Url {
        Url::parse(text).expect("a URL")
    }

    #[test]
    fn the_pages_of_the_app_load() {
        for page in [
            "tauri://localhost/index.html?window=01ARZ3NDEKTSV4RRFFQ69G5FAV",
            "http://tauri.localhost/index.html",
            "about:blank",
        ] {
            assert_eq!(navigation(&url(page)), Navigation::Allow, "{page}");
        }
        // The dev server counts in a debug build only, and then only its own port.
        assert_eq!(
            navigation(&url("http://localhost:1420/index.html")) == Navigation::Allow,
            cfg!(debug_assertions)
        );
        assert_eq!(navigation(&url("http://localhost:8080/")), Navigation::Deny);
    }

    #[test]
    fn a_link_opens_outside_the_app_only_for_three_schemes() {
        assert_eq!(
            navigation(&url("nh://open?url=https%3A%2F%2Fexample.com%2Fa%3Fb%3D1")),
            Navigation::OpenOutside("https://example.com/a?b=1".to_owned())
        );
        assert_eq!(
            navigation(&url("nh://open?url=mailto%3Aa%40example.com")),
            Navigation::OpenOutside("mailto:a@example.com".to_owned())
        );
        assert_eq!(
            navigation(&url("nh://open?url=http%3A%2F%2Fexample.com")),
            Navigation::OpenOutside("http://example.com/".to_owned())
        );
        for refused in [
            "nh://open?url=file%3A%2F%2F%2Fetc%2Fpasswd",
            "nh://open?url=javascript%3Aalert(1)",
            "nh://open?url=ftp%3A%2F%2Fexample.com",
            "nh://open?url=not-an-address",
            "nh://open",
            "nh://blob/abc",
        ] {
            assert_eq!(navigation(&url(refused)), Navigation::Deny, "{refused}");
        }
    }

    #[test]
    fn a_page_from_the_network_is_denied() {
        for outside in [
            "https://example.com/",
            "http://example.com/",
            "file:///etc/passwd",
            "data:text/html,<script>1</script>",
            "tauri://example.com/",
            "http://tauri.localhost.evil.example/",
        ] {
            assert_eq!(navigation(&url(outside)), Navigation::Deny, "{outside}");
        }
    }

    #[test]
    fn a_blob_address_names_one_blob() {
        let hash = "a".repeat(64);
        assert_eq!(blob_name(&format!("nh://blob/{hash}")), Some(hash.clone()));
        for other in [
            "nh://blob/",
            "nh://blob",
            "nh://blob/a/b",
            "nh://other/abc",
            "https://blob/abc",
            "not a url",
        ] {
            assert_eq!(blob_name(other), None, "{other}");
        }
    }
}
