use super::*;

fn html(text: &str) -> String {
    markdown(text).html
}

#[test]
fn text_becomes_paragraphs_lists_and_headings() {
    let out = html(
        "# Title\n\nSome *emphasis* and **strong** text.\n\n- one\n- two\n\n1. first\n2. second\n",
    );
    assert!(out.contains("<h1>Title</h1>"), "{out}");
    assert!(out.contains("<em>emphasis</em>"), "{out}");
    assert!(out.contains("<strong>strong</strong>"), "{out}");
    assert!(out.contains("<ul>"), "{out}");
    assert!(out.contains("<ol>"), "{out}");
}

#[test]
fn a_table_keeps_its_alignment() {
    let out = html("| a | b |\n|:--|--:|\n| 1 | 2 |\n");
    assert!(out.contains("<table>"), "{out}");
    assert!(out.contains("style=\"text-align: left\""), "{out}");
    assert!(out.contains("style=\"text-align: right\""), "{out}");
    // Any other style is gone.
    let out = html(
        "<table><tr><td style=\"position:fixed;top:0\">x</td><th style=\"text-align: left; color: red\">y</th></tr></table>",
    );
    assert!(!out.contains("style"), "{out}");
}

#[test]
fn a_code_block_is_highlighted_with_classes() {
    let out = html("```rust\nfn main() {}\n```\n");
    assert!(out.contains("<pre><code class=\"language-rust\">"), "{out}");
    assert!(out.contains("<span class=\"hl-kw\">fn</span>"), "{out}");
}

#[test]
fn the_text_of_a_code_block_is_escaped() {
    let out = html("```html\n<script>alert(1)</script>\n```\n");
    assert!(!out.contains("<script"), "{out}");
    assert!(out.contains("&lt;"), "{out}");
    // A block with no language is escaped text with no spans.
    let plain = html("```\n<b>x</b> & y\n```\n");
    assert!(plain.contains("&lt;b&gt;x&lt;/b&gt; &amp; y"), "{plain}");
    assert!(!plain.contains("<span"), "{plain}");
}

#[test]
fn a_language_name_with_odd_characters_makes_no_class() {
    let out = html("```ru\"st onclick=x\ncode\n```\n");
    assert!(!out.contains("onclick"), "{out}");
    let out = html("~~~\"><img src=x onerror=alert(1)>\ncode\n~~~\n");
    assert!(!out.contains("onerror"), "{out}");
}

#[test]
fn web_and_mail_links_open_through_the_core() {
    let out = html("[site](https://example.com/a?b=1&c=2) and [mail](mailto:me@example.com)");
    assert!(
        out.contains("href=\"nh://open?url=https%3A%2F%2Fexample.com%2Fa%3Fb%3D1%26c%3D2\""),
        "{out}"
    );
    assert!(
        out.contains("href=\"nh://open?url=mailto%3Ame%40example.com\""),
        "{out}"
    );
    assert!(out.contains("rel=\"noopener noreferrer\""), "{out}");
}

#[test]
fn other_links_lose_their_target() {
    for target in [
        "javascript:alert(1)",
        "JaVaScRiPt:alert(1)",
        "data:text/html,<script>alert(1)</script>",
        "file:///etc/passwd",
        "ftp://example.com/x",
        "vbscript:x",
        "nh://open?url=javascript:alert(1)",
        "docs/page.md",
        "#anchor",
        "//evil.example/x",
        "/absolute/path",
        "http:nohost",
        " javascript:alert(1)",
    ] {
        let out = html(&format!("[x](<{target}>)"));
        assert!(!out.contains("href"), "{target}: {out}");
        assert!(
            out.contains(">x</a>") || out.contains("x"),
            "{target}: {out}"
        );
    }
}

#[test]
fn an_image_loads_only_from_the_blob_store() {
    let out = html("![chart](nh://blob/abc123-_)");
    assert!(out.contains("src=\"nh://blob/abc123-_\""), "{out}");
    for source in [
        "https://evil.example/pixel.png",
        "http://evil.example/pixel.png",
        "data:image/svg+xml;base64,PHN2Zz4=",
        "nh://blob/",
        "nh://blob/a/b",
        "nh://other/x",
        "file:///etc/passwd",
        "/relative.png",
    ] {
        let out = html(&format!("![alt](<{source}>)"));
        assert!(!out.contains("src="), "{source}: {out}");
    }
}

#[test]
fn html_in_the_text_is_cleaned() {
    for (input, forbidden) in [
        ("<script>alert(1)</script>", "script"),
        ("<img src=x onerror=alert(1)>", "onerror"),
        ("<svg onload=alert(1)><circle/></svg>", "svg"),
        ("<iframe src=\"https://evil.example\"></iframe>", "iframe"),
        ("<object data=x></object><embed src=x>", "embed"),
        (
            "<form action=x><input name=a><button>go</button></form>",
            "form",
        ),
        ("<style>body{display:none}</style>", "style"),
        (
            "<a href=\"javascript:alert(1)\" onclick=\"x()\">x</a>",
            "javascript",
        ),
        ("<div style=\"position:fixed\">x</div>", "style"),
        ("<p id=\"a\" onmouseover=\"x()\">x</p>", "onmouseover"),
        (
            "<math><mi xlink:href=\"javascript:x\">x</mi></math>",
            "javascript",
        ),
        ("<!-- comment --><b>x</b>", "comment"),
    ] {
        let out = html(input);
        assert!(!out.contains(forbidden), "{input}: {out}");
    }
}

#[test]
fn the_content_of_a_script_goes_away_with_it() {
    let out = html("before <script>alert('secret')</script> after");
    assert!(!out.contains("secret"), "{out}");
    assert!(out.contains("before"), "{out}");
    assert!(out.contains("after"), "{out}");
}

#[test]
fn a_class_must_be_one_that_the_renderer_writes() {
    let out = html(
        "<span class=\"fixed inset-0\">x</span><span class=\"hl-kw\">y</span><span class=\"hl-nope\">z</span><span class=\"hl-kw extra\">w</span>",
    );
    assert!(!out.contains("fixed"), "{out}");
    assert!(!out.contains("inset"), "{out}");
    assert!(out.contains("<span class=\"hl-kw\">y</span>"), "{out}");
    assert!(!out.contains("hl-nope"), "{out}");
    assert!(!out.contains("extra"), "{out}");
    let out = html("<p class=\"hl-kw\">x</p><code class=\"language-rust bad\">y</code>");
    assert!(!out.contains("class"), "{out}");
}

#[test]
fn a_task_list_has_no_check_box() {
    let out = html("- [x] done\n- [ ] todo\n");
    assert!(!out.contains("<input"), "{out}");
    assert!(out.contains("☑ done"), "{out}");
    assert!(out.contains("☐ todo"), "{out}");
}

#[test]
fn a_long_plan_is_cut_and_flagged() {
    let long = "word ".repeat(PLAN_CAP / 4);
    let result = markdown(&long);
    assert!(result.truncated);
    assert!(!markdown("short").truncated);
}

#[test]
fn an_ordered_list_start_is_a_number() {
    assert!(html("5. five\n6. six\n").contains("<ol start=\"5\">"));
    let out = html("<ol start=\"javascript:1\"><li>x</li></ol>");
    assert!(!out.contains("start"), "{out}");
}

#[test]
fn empty_text_gives_empty_html() {
    assert_eq!(html(""), "");
}
