//! `rich.diff`, `rich.markdown` and `rich.highlight`: the API answers with the expected
//! shapes, and untrusted text comes out safe.

mod core_support;

use core_support::{Harness, client::Client};
use nh_core::ErrorCode;
use serde_json::json;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_diff_of_two_texts() {
    let h = Harness::start();
    let result = h
        .call(
            "rich.diff",
            json!({
                "a": "fn a() {\n    one();\n}\n",
                "b": "fn a() {\n    two();\n}\n",
                "path": "src/lib.rs",
            }),
        )
        .await;
    let file = &result["files"][0];
    assert_eq!(file["path"], "src/lib.rs");
    assert_eq!(file["status"], "modified");
    assert_eq!(file["stats"], json!({ "add": 1, "del": 1 }));
    let lines = file["hunks"][0]["lines"].as_array().expect("lines");
    let kinds: Vec<&str> = lines
        .iter()
        .map(|l| l["kind"].as_str().expect("kind"))
        .collect();
    assert_eq!(kinds, ["ctx", "del", "add", "ctx"]);
    // The language comes from the path: `fn` is a keyword.
    assert_eq!(lines[0]["text"]["spans"][0], json!([0, 2, "kw"]));
    // The changed word is marked on both sides.
    assert_eq!(lines[1]["intra"], json!([[4, 7]]));
    assert_eq!(lines[2]["intra"], json!([[4, 7]]));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_diff_takes_the_language_by_name() {
    let h = Harness::start();
    let result = h
        .call(
            "rich.diff",
            json!({ "a": "let x = 1;\n", "b": "let x = 2;\n", "lang": "rust" }),
        )
        .await;
    let line = &result["files"][0]["hunks"][0]["lines"][1];
    assert!(!line["text"]["spans"].as_array().expect("spans").is_empty());
    // A language that does not exist gives plain lines, not an error.
    let result = h
        .call(
            "rich.diff",
            json!({ "a": "x\n", "b": "y\n", "lang": "klingon" }),
        )
        .await;
    assert_eq!(
        result["files"][0]["hunks"][0]["lines"][1]["text"]["spans"],
        json!([])
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_long_diff_is_cut_at_2000_lines() {
    let h = Harness::start();
    let b: String = (0..3000).map(|n| format!("line {n}\n")).collect();
    let result = h.call("rich.diff", json!({ "a": "", "b": b })).await;
    let file = &result["files"][0];
    assert_eq!(file["truncated"], true);
    assert_eq!(file["stats"]["add"], 3000);
    let shown: usize = file["hunks"]
        .as_array()
        .expect("hunks")
        .iter()
        .map(|h| h["lines"].as_array().expect("lines").len())
        .sum();
    assert_eq!(shown, 2000);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn markdown_comes_out_as_safe_html() {
    let h = Harness::start();
    let text = "# Plan\n\n<script>alert(1)</script><img src=x onerror=alert(1)>\n\n[a](javascript:alert(1)) [b](https://example.com/x)\n\n```rust\nfn a() {}\n```\n";
    let result = h.call("rich.markdown", json!({ "text": text })).await;
    let html = result["html"].as_str().expect("html");
    assert!(html.contains("<h1>Plan</h1>"), "{html}");
    assert!(html.contains("<span class=\"hl-kw\">fn</span>"), "{html}");
    assert!(
        html.contains("href=\"nh://open?url=https%3A%2F%2Fexample.com%2Fx\""),
        "{html}"
    );
    for forbidden in ["<script", "onerror", "javascript"] {
        assert!(!html.contains(forbidden), "{forbidden} in {html}");
    }
    assert_eq!(result["truncated"], false);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_plan_over_256_kilobytes_is_cut() {
    let h = Harness::start();
    let text = "word ".repeat(80_000);
    let result = h.call("rich.markdown", json!({ "text": text })).await;
    assert_eq!(result["truncated"], true);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn highlighted_text_has_utf16_offsets() {
    let h = Harness::start();
    let result = h
        .call(
            "rich.highlight",
            json!({ "text": "let s = \"😀\"; // é\n", "lang": "rust" }),
        )
        .await;
    assert_eq!(result["text"], "let s = \"😀\"; // é\n");
    let spans = result["spans"].as_array().expect("spans");
    let comment = spans.iter().find(|s| s[2] == "com").expect("a comment");
    let units: Vec<u16> = "let s = \"😀\"; // é\n".encode_utf16().collect();
    let start = comment[0].as_u64().expect("start") as usize;
    let end = comment[1].as_u64().expect("end") as usize;
    assert_eq!(
        String::from_utf16(&units[start..end]).expect("text"),
        "// é"
    );
    let plain = h
        .call(
            "rich.highlight",
            json!({ "text": "hello", "lang": "klingon" }),
        )
        .await;
    assert_eq!(plain, json!({ "text": "hello", "spans": [] }));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bad_parameters_are_refused() {
    let h = Harness::start();
    for (method, params) in [
        ("rich.diff", json!({ "a": "x" })),
        ("rich.markdown", json!({})),
        ("rich.highlight", json!({ "text": "x" })),
        ("rich.markdown", json!({ "text": 5 })),
    ] {
        let error = h.handle.rpc(method, params).await.expect_err("an error");
        assert_eq!(error.code, ErrorCode::InvalidParams, "{method}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_methods_are_for_the_ui_and_not_for_the_cli_socket() {
    let h = Harness::start();
    let mut c = Client::connect(&h.api_socket()).await;
    let reply = c.call("rich.markdown", json!({ "text": "**bold**" })).await;
    assert_eq!(
        reply["error"]["data"]["code"], "permission_denied",
        "{reply}"
    );
}
