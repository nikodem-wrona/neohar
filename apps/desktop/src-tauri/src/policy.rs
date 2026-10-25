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
/// build. Each system has one origin for the bundle, with no port.
fn is_app(url: &Url) -> bool {
    match url.scheme() {
        // The pages of a bundle on macOS and Linux.
        "tauri" => !cfg!(windows) && url.host_str() == Some("localhost") && url.port().is_none(),
        // The pages of a bundle on Windows. The app does not set `useHttpsScheme`.
        "http" if url.host_str() == Some("tauri.localhost") => {
            cfg!(windows) && url.port().is_none()
        }
        "http" => {
            cfg!(debug_assertions)
                && url.host_str() == Some(DEV_ORIGIN.1)
                && url.port() == Some(DEV_ORIGIN.2)
        }
        "about" => url.as_str() == "about:blank",
        _ => false,
    }
}

/// Decides what a webview does with a navigation. `gesture` is the token that the host gave the
/// webview (see `windows::gesture`).
///
/// - A page of the app loads.
/// - `nh://open?url=<address>&gesture=<token>` opens the address outside the app if its scheme is
///   `http`, `https` or `mailto`. The token must be the one of the host: only the script of the
///   page has it, and the page adds it in the handler of a click. A refresh, a form or another
///   navigation in hostile HTML has no token, so the host opens nothing without a click.
/// - Everything else is denied: a link in rich content must not take the webview away from the
///   app, and the webview must not load a page from the network.
pub fn navigation(url: &Url, gesture: &str) -> Navigation {
    if is_app(url) {
        return Navigation::Allow;
    }
    if url.scheme() == "nh" && url.host_str() == Some("open") {
        let (mut target, mut token, mut other) = (None, None, false);
        for (key, value) in url.query_pairs() {
            match key.as_ref() {
                "url" if target.is_none() => target = Some(value.into_owned()),
                "gesture" if token.is_none() => token = Some(value.into_owned()),
                _ => other = true,
            }
        }
        if other || gesture.is_empty() || token.as_deref() != Some(gesture) {
            return Navigation::Deny;
        }
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

    /// The token of the host in these tests.
    const TOKEN: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV01ARZ3NDEKTSV4RRFFQ69G5FAW";

    fn go(text: &str) -> Navigation {
        navigation(&url(text), TOKEN)
    }

    #[test]
    fn the_pages_of_the_app_load() {
        assert_eq!(go("about:blank"), Navigation::Allow);
        // The origin of a bundle on macOS and Linux, and the one on Windows, count only there.
        assert_eq!(
            go("tauri://localhost/index.html?window=01ARZ3NDEKTSV4RRFFQ69G5FAV")
                == Navigation::Allow,
            !cfg!(windows)
        );
        assert_eq!(
            go("http://tauri.localhost/index.html") == Navigation::Allow,
            cfg!(windows)
        );
        // The dev server counts in a debug build only, and then only its own port.
        assert_eq!(
            go("http://localhost:1420/index.html") == Navigation::Allow,
            cfg!(debug_assertions)
        );
        for other in [
            "http://localhost:8080/",
            "tauri://localhost:8080/index.html",
            "http://tauri.localhost:8080/index.html",
            "https://tauri.localhost/index.html",
        ] {
            assert_eq!(go(other), Navigation::Deny, "{other}");
        }
    }

    #[test]
    fn a_link_opens_outside_the_app_only_for_three_schemes() {
        assert_eq!(
            go(&format!(
                "nh://open?url=https%3A%2F%2Fexample.com%2Fa%3Fb%3D1&gesture={TOKEN}"
            )),
            Navigation::OpenOutside("https://example.com/a?b=1".to_owned())
        );
        assert_eq!(
            go(&format!(
                "nh://open?url=mailto%3Aa%40example.com&gesture={TOKEN}"
            )),
            Navigation::OpenOutside("mailto:a@example.com".to_owned())
        );
        assert_eq!(
            go(&format!(
                "nh://open?url=http%3A%2F%2Fexample.com&gesture={TOKEN}"
            )),
            Navigation::OpenOutside("http://example.com/".to_owned())
        );
        for refused in [
            "nh://open?url=file%3A%2F%2F%2Fetc%2Fpasswd",
            "nh://open?url=javascript%3Aalert(1)",
            "nh://open?url=ftp%3A%2F%2Fexample.com",
            "nh://open?url=not-an-address",
            "nh://open?",
        ] {
            let with_token = format!("{refused}&gesture={TOKEN}");
            assert_eq!(go(&with_token), Navigation::Deny, "{with_token}");
        }
        assert_eq!(go("nh://open"), Navigation::Deny);
        assert_eq!(go("nh://blob/abc"), Navigation::Deny);
    }

    #[test]
    fn a_link_opens_only_with_the_token_of_the_host() {
        let link = "nh://open?url=https%3A%2F%2Fexample.com%2F";
        // A refresh or a form of hostile HTML has no token, or a wrong one.
        for refused in [
            link.to_owned(),
            format!("{link}&gesture="),
            format!("{link}&gesture=01ARZ3NDEKTSV4RRFFQ69G5FAV"),
            format!("{link}&gesture={TOKEN}x"),
            // A second token, a second address or another key is not a link of the page.
            format!("{link}&gesture={TOKEN}&gesture={TOKEN}"),
            format!("{link}&gesture={TOKEN}&url=https%3A%2F%2Fevil.example"),
            format!("{link}&gesture={TOKEN}&x=1"),
        ] {
            assert_eq!(go(&refused), Navigation::Deny, "{refused}");
        }
        // A host without a token opens nothing.
        assert_eq!(
            navigation(&url(&format!("{link}&gesture=")), ""),
            Navigation::Deny
        );
        assert_eq!(
            go(&format!("{link}&gesture={TOKEN}")),
            Navigation::OpenOutside("https://example.com/".to_owned())
        );
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
            assert_eq!(go(outside), Navigation::Deny, "{outside}");
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
