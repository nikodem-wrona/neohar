//! Generators of terminal output for property tests.
//!
//! The streams mix what real programs send (text, colours, cursor movement, scrolling, screen
//! switches) with what they should not (invalid UTF-8, sequences cut at any byte). A stream is a
//! list of [`Op`]s: bytes for the engine, and now and then a change of size.
//!
//! [`vt`] has a second kind of generator: a corpus of terminal output that is reproducible from a
//! seed and covers every piece of terminal state. The `nh-fixture gen-vt` command writes it.
//!
//! [`tui`] and [`echo_seq`] are two programs for the chaos test (`nh-chaos`): a full-screen program that
//! animates and redraws on resize, and a program that logs every input byte with a number.

use proptest::prelude::*;

pub mod echo_seq;
pub mod tui;
pub mod vt;

pub use vt::{corpus, corpus_from};

/// One step of a stream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Op {
    /// Output of the child.
    Bytes(Vec<u8>),
    /// The pane changes size.
    Resize { cols: u16, rows: u16 },
}

fn seq(text: String) -> Vec<u8> {
    text.into_bytes()
}

fn csi(params: impl Strategy<Value = String>, fin: char) -> impl Strategy<Value = Vec<u8>> {
    params.prop_map(move |p| seq(format!("\x1b[{p}{fin}")))
}

fn num(max: u32) -> impl Strategy<Value = u32> {
    prop_oneof![3 => 1..=max.min(12), 1 => 1..=max]
}

fn sgr_param() -> impl Strategy<Value = String> {
    let simple = prop::sample::select(vec![
        "0", "1", "2", "3", "4", "7", "8", "9", "21", "22", "23", "24", "27", "28", "29", "39",
        "49", "53", "55", "59", "4:1", "4:2", "4:3", "4:4", "4:5",
    ])
    .prop_map(String::from);
    prop_oneof![
        4 => simple,
        2 => (30u8..=37).prop_map(|n| n.to_string()),
        2 => (40u8..=47).prop_map(|n| n.to_string()),
        1 => (90u8..=97).prop_map(|n| n.to_string()),
        1 => (100u8..=107).prop_map(|n| n.to_string()),
        2 => any::<u8>().prop_map(|n| format!("38;5;{n}")),
        2 => any::<u8>().prop_map(|n| format!("48;5;{n}")),
        1 => any::<u8>().prop_map(|n| format!("58;5;{n}")),
        2 => (any::<u8>(), any::<u8>(), any::<u8>()).prop_map(|(r, g, b)| format!("38;2;{r};{g};{b}")),
        2 => (any::<u8>(), any::<u8>(), any::<u8>()).prop_map(|(r, g, b)| format!("48;2;{r};{g};{b}")),
        1 => (any::<u8>(), any::<u8>(), any::<u8>()).prop_map(|(r, g, b)| format!("58;2;{r};{g};{b}")),
    ]
}

fn mode() -> impl Strategy<Value = u32> {
    prop::sample::select(vec![
        1, 6, 7, 12, 25, 47, 1000, 1002, 1003, 1004, 1005, 1006, 1007, 1047, 1049, 2004,
    ])
}

fn color_spec() -> impl Strategy<Value = String> {
    (any::<u8>(), any::<u8>(), any::<u8>())
        .prop_map(|(r, g, b)| format!("rgb:{r:02x}/{g:02x}/{b:02x}"))
}

/// Printable text in several scripts, with wide characters, emoji and combining marks.
pub fn text() -> impl Strategy<Value = Vec<u8>> {
    let tricky = prop::sample::select(vec![
        "日本語のテキスト",
        "😀",
        "👍🏽",
        "e\u{301}",
        "👨\u{200d}👩\u{200d}👧",
        "Ａｂｃ",
        "ｱｲｳ",
        "ñandú",
        "a\u{200b}b",
        "ア\u{3099}",
        "\u{1f1e9}\u{1f1ea}",
        "╭─╮│╰╯",
    ])
    .prop_map(|s| s.to_owned());
    prop_oneof![
        4 => "[ -~]{1,40}",
        2 => tricky,
        1 => "\\PC{1,8}",
    ]
    .prop_map(String::into_bytes)
}

/// One escape sequence or control character.
pub fn escape() -> impl Strategy<Value = Vec<u8>> {
    let fixed = prop::sample::select(vec![
        "\r\n",
        "\n",
        "\r",
        "\t",
        "\x08",
        "\x07",
        "\x0b",
        "\x0c",
        "\x1bD",
        "\x1bE",
        "\x1bM",
        "\x1bH",
        "\x1b7",
        "\x1b8",
        "\x1b=",
        "\x1b>",
        "\x1b(0",
        "\x1b(B",
        "\x0e",
        "\x0f",
        "\x1b[s",
        "\x1b[u",
        "\x1b[3g",
        "\x1b[0g",
        "\x1b[r",
        "\x1b[>1u",
        "\x1b[>5u",
        "\x1b[<u",
        "\x1b[=3;2u",
        "\x1b]104\x07",
        "\x1b]110\x07",
        "\x1b]111\x07",
        "\x1b]8;;http://example.com\x1b\\",
        "\x1b]8;;\x1b\\",
        "\x1bc",
        "\x1b[!p",
    ])
    .prop_map(|s| seq(s.to_owned()));
    prop_oneof![
        3 => fixed,
        6 => prop::collection::vec(sgr_param(), 1..4).prop_flat_map(|p| csi(Just(p.join(";")), 'm')),
        3 => (num(70), num(250)).prop_flat_map(|(r, c)| csi(Just(format!("{r};{c}")), 'H')),
        1 => (num(70), num(250)).prop_flat_map(|(r, c)| csi(Just(format!("{r};{c}")), 'f')),
        2 => num(30).prop_flat_map(|n| csi(Just(n.to_string()), 'A')),
        2 => num(30).prop_flat_map(|n| csi(Just(n.to_string()), 'B')),
        2 => num(90).prop_flat_map(|n| csi(Just(n.to_string()), 'C')),
        2 => num(90).prop_flat_map(|n| csi(Just(n.to_string()), 'D')),
        1 => num(250).prop_flat_map(|n| csi(Just(n.to_string()), 'G')),
        1 => num(70).prop_flat_map(|n| csi(Just(n.to_string()), 'd')),
        2 => (0u8..=3).prop_flat_map(|n| csi(Just(n.to_string()), 'J')),
        2 => (0u8..=2).prop_flat_map(|n| csi(Just(n.to_string()), 'K')),
        1 => num(40).prop_flat_map(|n| csi(Just(n.to_string()), 'X')),
        2 => num(10).prop_flat_map(|n| csi(Just(n.to_string()), 'L')),
        2 => num(10).prop_flat_map(|n| csi(Just(n.to_string()), 'M')),
        1 => num(10).prop_flat_map(|n| csi(Just(n.to_string()), '@')),
        1 => num(10).prop_flat_map(|n| csi(Just(n.to_string()), 'P')),
        2 => num(10).prop_flat_map(|n| csi(Just(n.to_string()), 'S')),
        2 => num(10).prop_flat_map(|n| csi(Just(n.to_string()), 'T')),
        1 => num(10).prop_flat_map(|n| csi(Just(n.to_string()), 'I')),
        1 => num(10).prop_flat_map(|n| csi(Just(n.to_string()), 'Z')),
        2 => (num(30), num(40)).prop_flat_map(|(a, b)| csi(Just(format!("{a};{}", a + b)), 'r')),
        3 => mode().prop_flat_map(|m| csi(Just(format!("?{m}")), 'h')),
        3 => mode().prop_flat_map(|m| csi(Just(format!("?{m}")), 'l')),
        1 => prop::sample::select(vec!["4", "20"]).prop_flat_map(|m| csi(Just(m.to_owned()), 'h')),
        1 => prop::sample::select(vec!["4", "20"]).prop_flat_map(|m| csi(Just(m.to_owned()), 'l')),
        1 => (0u8..=6).prop_flat_map(|n| csi(Just(format!("{n} ")), 'q')),
        1 => "[a-z ]{0,12}".prop_map(|t| seq(format!("\x1b]0;{t}\x07"))),
        1 => "[a-z ]{0,12}".prop_map(|t| seq(format!("\x1b]2;{t}\x1b\\"))),
        1 => (any::<u8>(), color_spec()).prop_map(|(n, c)| seq(format!("\x1b]4;{n};{c}\x07"))),
        1 => color_spec().prop_map(|c| seq(format!("\x1b]10;{c}\x07"))),
        1 => color_spec().prop_map(|c| seq(format!("\x1b]11;{c}\x07"))),
        1 => color_spec().prop_map(|c| seq(format!("\x1b]12;{c}\x07"))),
        1 => prop::sample::select(vec!["\x1b[c", "\x1b[>c", "\x1b[=c", "\x1b[>q", "\x1b[6n", "\x1b[5n", "\x1b[?2004$p", "\x1b]11;?\x07", "\x1b[14t", "\x1b[18t"])
            .prop_map(|s| seq(s.to_owned())),
        1 => prop::collection::vec(any::<u8>(), 1..8),
    ]
}

/// A piece of output: text or an escape sequence.
pub fn piece() -> impl Strategy<Value = Vec<u8>> {
    prop_oneof![3 => text(), 4 => escape()]
}

/// Output of a program: many pieces.
pub fn output(max_pieces: usize) -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(piece(), 0..=max_pieces).prop_map(|p| p.concat())
}

/// Cuts `bytes` at the given fractions of its length. A cut can fall inside a sequence or a
/// character.
pub fn cut(bytes: &[u8], cuts: &[prop::sample::Index]) -> Vec<Vec<u8>> {
    let mut points: Vec<usize> = cuts.iter().map(|c| c.index(bytes.len() + 1)).collect();
    points.push(0);
    points.push(bytes.len());
    points.sort_unstable();
    points.dedup();
    points
        .windows(2)
        .map(|w| bytes[w[0]..w[1]].to_vec())
        .collect()
}

/// A stream of operations: output cut into chunks, and now and then a resize.
pub fn ops(max_ops: usize) -> impl Strategy<Value = Vec<Op>> {
    let bytes = (
        output(24),
        prop::collection::vec(any::<prop::sample::Index>(), 0..4),
    )
        .prop_map(|(out, cuts)| {
            cut(&out, &cuts)
                .into_iter()
                .map(Op::Bytes)
                .collect::<Vec<_>>()
        });
    let resize = (2u16..=120, 1u16..=40).prop_map(|(cols, rows)| vec![Op::Resize { cols, rows }]);
    prop::collection::vec(prop_oneof![7 => bytes, 1 => resize], 1..=max_ops)
        .prop_map(|v| v.concat())
}
