//! The listener of the phone and Tailscale against a real core: the state in
//! `phone_status`, the origin from the name in the tailnet, `phone.publish`, `phone.unpublish`,
//! the hints and the lines of the doctor.
//!
//! Every test gives the listener the fake CLI of `tests/support/fake-tailscale.sh` in a folder of
//! its own, so no test reaches the Tailscale of the machine.

mod core_support;

use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use core_support::{Harness, TIMEOUT};
use nh_core::ErrorCode;
use nh_proto::api::model::{PhoneStatusValue, TailscaleState};
use nh_web::{PhoneService, TailscaleCli};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

/// `tailscale status --json` of a Mac in a tailnet with HTTPS certificates.
const RUNNING: &str = include_str!("../../nh-web/tests/fixtures/status-running.json");

/// The same in a tailnet without HTTPS certificates.
const NO_HTTPS: &str = include_str!("../../nh-web/tests/fixtures/status-no-https.json");

const ORIGIN: &str = "https://studio.example.ts.net:8443";

/// The fake CLI in a folder of its own, with the files that a test chooses.
struct Fake {
    dir: PathBuf,
}

impl Fake {
    fn new(h: &Harness) -> Self {
        let dir = h.dir.join("tailscale");
        std::fs::create_dir_all(&dir).expect("the folder of the fake");
        // A link, not a copy: macOS checks each new program before its first start, and that
        // takes seconds while other builds run. The script finds its folder from the link.
        let script = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/support/fake-tailscale.sh")
            .canonicalize()
            .expect("the fake");
        std::os::unix::fs::symlink(&script, dir.join("tailscale")).expect("the link to the fake");
        Self { dir }
    }

    fn cli(&self) -> TailscaleCli {
        TailscaleCli::At(self.dir.join("tailscale"))
    }

    fn write(&self, name: &str, text: &str) {
        std::fs::write(self.dir.join(name), text).expect("a file of the fake");
    }

    /// The arguments of each run of the fake.
    fn calls(&self) -> Vec<String> {
        std::fs::read_to_string(self.dir.join("calls"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    /// The runs that change the mapping.
    fn changes(&self) -> Vec<String> {
        self.calls()
            .into_iter()
            .filter(|call| call.starts_with("serve ") && call != "serve status --json")
            .collect()
    }

    fn serve(&self) -> Value {
        std::fs::read_to_string(self.dir.join("serve.json"))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_else(|| json!({}))
    }
}

/// A free port of `127.0.0.1`.
fn free_port() -> u16 {
    let free = std::net::TcpListener::bind("127.0.0.1:0").expect("a free port");
    free.local_addr().expect("an address").port()
}

/// Turns on the listener on a free port and returns the port.
async fn enable(h: &Harness) -> u16 {
    let port = free_port();
    h.call("config.set", json!({ "key": "phone.port", "value": port }))
        .await;
    h.call(
        "config.set",
        json!({ "key": "phone.enabled", "value": true }),
    )
    .await;
    port
}

/// Waits until `phone_status` satisfies `pred`.
async fn status_until(
    h: &Harness,
    what: &str,
    pred: impl Fn(&PhoneStatusValue) -> bool,
) -> PhoneStatusValue {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let status = h.handle.phone_status();
        if pred(&status) {
            return status;
        }
        assert!(Instant::now() < deadline, "waited for {what}: {status:?}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn state_of(status: &PhoneStatusValue) -> Option<TailscaleState> {
    status.tailscale.as_ref().map(|t| t.state)
}

/// The status of a GET of `/` with this `Host`.
async fn get_root(port: u16, host: &str) -> u16 {
    let mut stream = TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("a connection");
    let request = format!("GET / HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n");
    stream
        .write_all(request.as_bytes())
        .await
        .expect("the request");
    let mut answer = String::new();
    let _ = stream.read_to_string(&mut answer).await;
    answer
        .split(' ')
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or(0)
}

/// The status of the doctor line `id`.
async fn doctor_line(h: &Harness, id: &str) -> Value {
    let report = h.call("doctor.run", json!({})).await;
    report["checks"]
        .as_array()
        .and_then(|checks| checks.iter().find(|c| c["id"] == id).cloned())
        .unwrap_or_else(|| panic!("no doctor line {id}: {report}"))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn publish_maps_the_https_port_to_the_listener_and_unpublish_removes_it() {
    let h = Harness::start_layout();
    let fake = Fake::new(&h);
    fake.write("status.json", RUNNING);
    let _phone = PhoneService::start_with(h.handle.clone(), fake.cli());
    let port = enable(&h).await;

    let before = status_until(&h, "Tailscale that runs", |s| {
        state_of(s) == Some(TailscaleState::Running)
    })
    .await;
    assert_eq!(before.port, Some(port));
    assert_eq!(
        before.origin.as_deref(),
        Some(ORIGIN),
        "the name gives the origin"
    );
    assert!(
        before
            .hint
            .as_deref()
            .is_some_and(|h| h.contains("Publish")),
        "{before:?}"
    );
    assert!(!before.tailscale.as_ref().expect("Tailscale").published);
    // A QR code that leads nowhere helps no one.
    let refused = h.handle.rpc("phone.pair_begin", json!({})).await;
    assert_eq!(
        refused.expect_err("not published").code,
        ErrorCode::Unsupported
    );

    let published = h.call("phone.publish", json!({})).await;
    assert_eq!(published["tailscale"]["published"], true, "{published}");
    assert_eq!(published["origin"], ORIGIN);
    assert!(published.get("hint").is_none(), "{published}");
    assert_eq!(
        published["summary"],
        format!("Published with Tailscale at {ORIGIN}.")
    );
    assert_eq!(
        fake.changes(),
        [format!(
            "serve --bg --yes --https=8443 http://127.0.0.1:{port}"
        )]
    );
    assert_eq!(
        fake.serve()["Web"]["studio.example.ts.net:8443"]["Handlers"]["/"]["Proxy"],
        format!("http://127.0.0.1:{port}")
    );
    // `tailscale serve` passes the host of the tailnet, and the listener takes it.
    assert_eq!(get_root(port, "studio.example.ts.net:8443").await, 200);
    assert_eq!(get_root(port, "evil.example:8443").await, 403);
    let offer = h.call("phone.pair_begin", json!({})).await;
    assert!(
        offer["url"]
            .as_str()
            .is_some_and(|url| url.starts_with(&format!("{ORIGIN}/pair?code="))),
        "{offer}"
    );

    // A second publish changes nothing.
    h.call("phone.publish", json!({})).await;
    assert_eq!(fake.changes().len(), 1);
    assert_eq!(doctor_line(&h, "tailscale_serve").await["status"], "ok");
    assert_eq!(doctor_line(&h, "tailscale_https").await["status"], "ok");
    assert_eq!(doctor_line(&h, "tailscale_funnel").await["status"], "ok");

    let unpublished = h.call("phone.unpublish", json!({})).await;
    assert_eq!(
        unpublished["tailscale"]["published"], false,
        "{unpublished}"
    );
    assert_eq!(
        fake.changes().last().map(String::as_str),
        Some("serve --yes --https=8443 off")
    );
    assert_eq!(fake.serve(), json!({}));
    assert_eq!(doctor_line(&h, "tailscale_serve").await["status"], "warn");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_tailnet_without_https_certificates_gives_the_hint_and_no_publish() {
    let h = Harness::start_layout();
    let fake = Fake::new(&h);
    fake.write("status.json", NO_HTTPS);
    let _phone = PhoneService::start_with(h.handle.clone(), fake.cli());
    let port = enable(&h).await;

    let status = status_until(&h, "Tailscale without certificates", |s| {
        state_of(s) == Some(TailscaleState::Running)
    })
    .await;
    assert_eq!(status.origin, None, "no certificate: no HTTPS origin");
    assert!(
        status
            .hint
            .as_deref()
            .is_some_and(|hint| hint.contains("Turn on HTTPS certificates")),
        "{status:?}"
    );
    assert_eq!(get_root(port, "studio.example.ts.net:8443").await, 403);

    let refused = h.handle.rpc("phone.publish", json!({})).await;
    let error = refused.expect_err("no certificates");
    assert_eq!(error.code, ErrorCode::Unsupported);
    assert!(error.message.contains("HTTPS certificates"), "{error:?}");
    assert!(fake.changes().is_empty(), "no serve without certificates");
    assert_eq!(doctor_line(&h, "tailscale").await["status"], "ok");
    assert_eq!(doctor_line(&h, "tailscale_https").await["status"], "fail");

    // The person turns on the certificates. The next read of Tailscale gives the origin.
    fake.write("status.json", RUNNING);
    h.call("doctor.run", json!({})).await;
    let status = h.handle.phone_status();
    assert_eq!(status.origin.as_deref(), Some(ORIGIN), "{status:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_funnel_on_the_port_fails_in_the_doctor_and_stops_the_pairing() {
    let h = Harness::start_layout();
    let fake = Fake::new(&h);
    fake.write("status.json", RUNNING);
    let _phone = PhoneService::start_with(h.handle.clone(), fake.cli());
    let port = enable(&h).await;
    fake.write(
        "serve.json",
        &json!({
            "TCP": { "8443": { "HTTPS": true } },
            "Web": { "studio.example.ts.net:8443": {
                "Handlers": { "/": { "Proxy": format!("http://127.0.0.1:{port}") } }
            } },
            "AllowFunnel": { "studio.example.ts.net:8443": true }
        })
        .to_string(),
    );

    let funnel = doctor_line(&h, "tailscale_funnel").await;
    assert_eq!(funnel["status"], "fail", "{funnel}");
    assert!(
        funnel["hint"]
            .as_str()
            .is_some_and(|hint| hint.contains("tailscale funnel --https=8443 off")),
        "{funnel}"
    );
    let status = h.handle.phone_status();
    assert!(status.tailscale.as_ref().is_some_and(|t| t.funnel));
    assert!(
        status.hint.as_deref().is_some_and(|h| h.contains("Funnel")),
        "{status:?}"
    );
    let refused = h.handle.rpc("phone.pair_begin", json!({})).await;
    assert_eq!(refused.expect_err("a Funnel").code, ErrorCode::Unsupported);
    assert!(
        fake.calls().iter().all(|call| !call.contains("funnel")),
        "neohar never runs tailscale funnel"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_mapping_of_something_else_stays_as_it_is() {
    let h = Harness::start_layout();
    let fake = Fake::new(&h);
    fake.write("status.json", RUNNING);
    let other = json!({
        "TCP": { "8443": { "HTTPS": true } },
        "Web": { "studio.example.ts.net:8443": {
            "Handlers": { "/": { "Proxy": "http://127.0.0.1:3000" } }
        } }
    });
    fake.write("serve.json", &other.to_string());
    let _phone = PhoneService::start_with(h.handle.clone(), fake.cli());
    enable(&h).await;

    let status = status_until(&h, "Tailscale that runs", |s| {
        state_of(s) == Some(TailscaleState::Running)
    })
    .await;
    assert!(
        status
            .hint
            .as_deref()
            .is_some_and(|hint| hint.contains("serves something else")),
        "{status:?}"
    );
    let refused = h.handle.rpc("phone.publish", json!({})).await;
    assert_eq!(
        refused.expect_err("the port is taken").code,
        ErrorCode::Unsupported
    );
    h.call("phone.unpublish", json!({})).await;
    assert!(fake.changes().is_empty(), "{:?}", fake.calls());
    assert_eq!(fake.serve(), other);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_cli_a_daemon_that_does_not_answer_and_a_failed_serve() {
    let h = Harness::start_layout();
    let fake = Fake::new(&h);
    let _phone = PhoneService::start_with(
        h.handle.clone(),
        TailscaleCli::At(fake.dir.join("not-there")),
    );
    enable(&h).await;
    let status = status_until(&h, "no Tailscale", |s| {
        state_of(s) == Some(TailscaleState::Missing)
    })
    .await;
    assert!(
        status
            .hint
            .as_deref()
            .is_some_and(|hint| hint.contains("Install Tailscale")),
        "{status:?}"
    );
    let line = doctor_line(&h, "tailscale").await;
    assert_eq!(line["status"], "fail");
    assert_eq!(line["detail"], "is not installed");
    drop(_phone);

    // The fake without `status.json` fails as a CLI without its daemon does. The old listener
    // closes its port while it stops, so the new one takes another port.
    let _phone = PhoneService::start_with(h.handle.clone(), fake.cli());
    enable(&h).await;
    status_until(&h, "a daemon that does not answer", |s| {
        state_of(s) == Some(TailscaleState::Unreachable)
    })
    .await;
    assert_eq!(
        doctor_line(&h, "tailscale").await["detail"],
        "does not answer"
    );

    fake.write("status.json", RUNNING);
    fake.write(
        "fail",
        "error: serve config denied by a policy of the tailnet",
    );
    let failed = h.handle.rpc("phone.publish", json!({})).await;
    let error = failed.expect_err("the serve fails");
    assert_eq!(error.code, ErrorCode::Internal);
    assert!(error.message.contains("denied by a policy"), "{error:?}");
    let status = h.handle.phone_status();
    assert!(!status.tailscale.as_ref().expect("Tailscale").published);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn phone_url_replaces_tailscale() {
    let h = Harness::start_layout();
    let fake = Fake::new(&h);
    fake.write("status.json", RUNNING);
    let _phone = PhoneService::start_with(h.handle.clone(), fake.cli());
    h.call(
        "config.set",
        json!({ "key": "phone.url", "value": "https://mac.example.org:9443/" }),
    )
    .await;
    let port = enable(&h).await;
    let status = status_until(&h, "the origin of phone.url", |s| {
        s.origin.as_deref() == Some("https://mac.example.org:9443")
    })
    .await;
    assert_eq!(status.tailscale, None, "{status:?}");
    assert_eq!(status.hint, None);
    assert_eq!(
        status.summary,
        format!("Listens on 127.0.0.1:{port} for https://mac.example.org:9443.")
    );
    let refused = h.handle.rpc("phone.publish", json!({})).await;
    let error = refused.expect_err("phone.url is set");
    assert!(error.message.contains("phone.url"), "{error:?}");
    let report = h.call("doctor.run", json!({})).await;
    assert!(
        report["checks"]
            .as_array()
            .expect("checks")
            .iter()
            .all(|c| !c["id"].as_str().unwrap_or("").starts_with("tailscale")),
        "{report}"
    );
    assert!(fake.calls().is_empty(), "{:?}", fake.calls());
}

/// Waits until the runs of the fake that change the mapping satisfy `pred`.
async fn changes_until(fake: &Fake, what: &str, pred: impl Fn(&[String]) -> bool) -> Vec<String> {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let changes = fake.changes();
        if pred(&changes) {
            return changes;
        }
        assert!(Instant::now() < deadline, "waited for {what}: {changes:?}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

const OFF: &str = "serve --yes --https=8443 off";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_mapping_lives_only_as_long_as_the_listener() {
    let h = Harness::start_layout();
    let fake = Fake::new(&h);
    fake.write("status.json", RUNNING);
    let phone = PhoneService::start_with(h.handle.clone(), fake.cli());
    let port = enable(&h).await;
    status_until(&h, "Tailscale that runs", |s| {
        state_of(s) == Some(TailscaleState::Running)
    })
    .await;
    h.call("phone.publish", json!({})).await;
    let publish = format!("serve --bg --yes --https=8443 http://127.0.0.1:{port}");
    assert_eq!(fake.changes(), std::slice::from_ref(&publish));

    // The listener stops: the port is free for any local user, so the mapping goes.
    h.call(
        "config.set",
        json!({ "key": "phone.enabled", "value": false }),
    )
    .await;
    changes_until(&fake, "the removal", |c| {
        c.last().map(String::as_str) == Some(OFF)
    })
    .await;
    assert_eq!(fake.serve(), json!({}));

    // The listener serves again: the mapping that the person asked for comes back.
    h.call(
        "config.set",
        json!({ "key": "phone.enabled", "value": true }),
    )
    .await;
    changes_until(&fake, "the mapping again", |c| {
        c.len() == 3 && c.last() == Some(&publish)
    })
    .await;
    status_until(&h, "a published listener", |s| {
        s.tailscale.as_ref().is_some_and(|t| t.published)
    })
    .await;

    // A quit of the app or of the daemon removes it too, before the stop ends.
    drop(phone);
    let changes = fake.changes();
    assert_eq!(changes.last().map(String::as_str), Some(OFF), "{changes:?}");

    // A new service publishes again, until `phone.unpublish`.
    let phone = PhoneService::start_with(h.handle.clone(), fake.cli());
    changes_until(&fake, "the mapping after a restart", |c| {
        c.len() == 5 && c.last() == Some(&publish)
    })
    .await;
    h.call("phone.unpublish", json!({})).await;
    assert_eq!(fake.serve(), json!({}));
    drop(phone);
    let _phone = PhoneService::start_with(h.handle.clone(), fake.cli());
    status_until(&h, "a listener that is not published", |s| {
        s.tailscale.as_ref().is_some_and(|t| !t.published)
    })
    .await;
    h.call("doctor.run", json!({})).await;
    assert_eq!(fake.changes().len(), 6, "{:?}", fake.changes());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_port_that_another_process_holds_loses_its_mapping() {
    let h = Harness::start_layout();
    let fake = Fake::new(&h);
    fake.write("status.json", RUNNING);
    // Another local user listens on the port, and a crash of neohar left the mapping to it.
    let other = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
    let port = other.local_addr().expect("an address").port();
    fake.write(
        "serve.json",
        &json!({
            "TCP": { "8443": { "HTTPS": true } },
            "Web": { "studio.example.ts.net:8443": {
                "Handlers": { "/": { "Proxy": format!("http://127.0.0.1:{port}") } }
            } }
        })
        .to_string(),
    );
    let _phone = PhoneService::start_with(h.handle.clone(), fake.cli());
    h.call("config.set", json!({ "key": "phone.port", "value": port }))
        .await;
    h.call(
        "config.set",
        json!({ "key": "phone.enabled", "value": true }),
    )
    .await;
    let status = status_until(&h, "a port that is not free", |s| {
        s.hint.as_deref().is_some_and(|h| h.contains("is not free"))
    })
    .await;
    assert!(!status.listening);
    changes_until(&fake, "the removal", |c| c == [OFF.to_owned()]).await;
    assert_eq!(fake.serve(), json!({}));
    drop(other);
}
