//! The synthetic corpus of `nh_fixture::vt`: it is reproducible, it has the size and the
//! chunks that it promises, and it covers every piece of terminal state.
//!
//! The tests read the corpus with a small scanner of their own that follows the rules of a
//! terminal parser. They do not use any other `nh-*` crate.

use std::collections::HashSet;

use nh_fixture::Op;
use nh_fixture::vt::{MAX_CHUNK, MAX_COLS, MAX_ROWS, MIN_COLS, MIN_ROWS, corpus, corpus_from};

const MIB: usize = 1 << 20;
const SEEDS: [u64; 5] = [0, 1, 2, 0xDEAD_BEEF, u64::MAX];

fn bytes_of(ops: &[Op]) -> Vec<u8> {
    ops.iter()
        .filter_map(|op| match op {
            Op::Bytes(b) => Some(b.as_slice()),
            Op::Resize { .. } => None,
        })
        .flatten()
        .copied()
        .collect()
}

fn total(ops: &[Op]) -> usize {
    ops.iter()
        .map(|op| match op {
            Op::Bytes(b) => b.len(),
            Op::Resize { .. } => 0,
        })
        .sum()
}

// ---------------------------------------------------------------------------------------------
// A scanner

#[derive(Clone, Debug, PartialEq, Eq)]
enum Tok {
    Text(Vec<u8>),
    /// A C0 control character or DEL.
    Ctl(u8),
    /// The bytes between `ESC [` and the final byte, both included.
    Csi(String),
    /// The bytes after `ESC`, up to the final byte (intermediates included).
    Esc(String),
    /// The bytes between `ESC ]` and the terminator.
    Osc(String),
    /// DCS, SOS, PM and APC strings.
    Str(String),
}

enum Seq {
    Done(Tok, usize),
    /// CAN or SUB ended it. The scanner continues at the index.
    Cancelled(usize),
    Incomplete,
}

fn lossy(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// The sequence that starts at the `ESC` at `i`.
fn sequence(b: &[u8], i: usize) -> Seq {
    let Some(&kind) = b.get(i + 1) else {
        return Seq::Incomplete;
    };
    match kind {
        b'[' => {
            let mut j = i + 2;
            while let Some(&d) = b.get(j) {
                match d {
                    0x20..=0x3f => j += 1,
                    0x40..=0x7e => return Seq::Done(Tok::Csi(lossy(&b[i + 2..=j])), j + 1),
                    0x18 | 0x1a => return Seq::Cancelled(j + 1),
                    _ => return Seq::Cancelled(j),
                }
            }
            Seq::Incomplete
        }
        b']' | b'P' | b'X' | b'^' | b'_' => {
            let mut j = i + 2;
            while let Some(&d) = b.get(j) {
                let done = |end: usize, next: usize| {
                    let body = lossy(&b[i + 2..end]);
                    let tok = if kind == b']' {
                        Tok::Osc(body)
                    } else {
                        Tok::Str(body)
                    };
                    Seq::Done(tok, next)
                };
                match d {
                    0x07 if kind == b']' => return done(j, j + 1),
                    0x1b => {
                        return match b.get(j + 1) {
                            Some(b'\\') => done(j, j + 2),
                            Some(_) => Seq::Cancelled(j),
                            None => Seq::Incomplete,
                        };
                    }
                    0x18 | 0x1a => return Seq::Cancelled(j + 1),
                    _ => j += 1,
                }
            }
            Seq::Incomplete
        }
        _ => {
            let mut j = i + 1;
            while let Some(&d) = b.get(j) {
                match d {
                    0x20..=0x2f => j += 1,
                    0x30..=0x7e => return Seq::Done(Tok::Esc(lossy(&b[i + 1..=j])), j + 1),
                    _ => return Seq::Cancelled(j),
                }
            }
            Seq::Incomplete
        }
    }
}

/// The tokens of `bytes`, and whether `bytes` ends at the ground state of the parser.
fn scan(bytes: &[u8]) -> (Vec<Tok>, bool) {
    let mut toks = Vec::new();
    let mut text = Vec::new();
    let mut i = 0;
    let mut ground = true;
    while i < bytes.len() {
        let b = bytes[i];
        if b == 0x1b {
            if !text.is_empty() {
                toks.push(Tok::Text(std::mem::take(&mut text)));
            }
            match sequence(bytes, i) {
                Seq::Done(tok, next) => {
                    toks.push(tok);
                    i = next;
                }
                Seq::Cancelled(next) => {
                    if matches!(bytes.get(next.wrapping_sub(1)), Some(0x18 | 0x1a)) {
                        toks.push(Tok::Ctl(bytes[next - 1]));
                    }
                    i = next;
                }
                Seq::Incomplete => {
                    ground = false;
                    break;
                }
            }
        } else if b < 0x20 || b == 0x7f {
            if !text.is_empty() {
                toks.push(Tok::Text(std::mem::take(&mut text)));
            }
            toks.push(Tok::Ctl(b));
            i += 1;
        } else {
            text.push(b);
            i += 1;
        }
    }
    if !text.is_empty() {
        toks.push(Tok::Text(text));
    }
    (toks, ground)
}

/// `true` if the bytes end inside a multi-byte character (invalid bytes elsewhere are fine).
fn ends_inside_a_character(mut bytes: &[u8]) -> bool {
    loop {
        match std::str::from_utf8(bytes) {
            Ok(_) => return false,
            Err(e) => match e.error_len() {
                None => return true,
                Some(n) => bytes = &bytes[e.valid_up_to() + n..],
            },
        }
    }
}

/// A CSI sequence taken apart.
#[derive(Debug)]
struct Csi {
    /// `<`, `=`, `>` or `?`, or empty.
    private: String,
    params: String,
    /// Intermediate bytes (`SP`, `!`, `$`, ...).
    inter: String,
    fin: char,
}

fn csi_parts(body: &str) -> Csi {
    let fin = body.chars().last().unwrap_or('?');
    let rest = &body[..body.len() - fin.len_utf8()];
    let private_len = rest.chars().take_while(|c| "<=>?".contains(*c)).count();
    let (private, rest) = rest.split_at(private_len);
    let params_len = rest
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == ';' || *c == ':')
        .count();
    let (params, inter) = rest.split_at(params_len);
    Csi {
        private: private.to_owned(),
        params: params.to_owned(),
        inter: inter.to_owned(),
        fin,
    }
}

/// The items of an SGR parameter list: `38;5;n` and `38;2;r;g;b` (also for 48 and 58) are one item.
fn sgr_items(params: &str) -> Vec<String> {
    let parts: Vec<&str> = params.split(';').collect();
    let mut items = Vec::new();
    let mut i = 0;
    while i < parts.len() {
        let p = parts[i];
        let take = match (p, parts.get(i + 1).copied()) {
            ("38" | "48" | "58", Some("5")) => 3,
            ("38" | "48" | "58", Some("2")) => 5,
            _ => 1,
        };
        let end = (i + take).min(parts.len());
        let item = parts[i..end].join(";");
        items.push(if item.is_empty() {
            "0".to_owned()
        } else {
            item
        });
        i = end;
    }
    items
}

/// What a corpus holds, by kind.
struct Facts {
    toks: Vec<Tok>,
    csi: Vec<Csi>,
    esc: HashSet<String>,
    osc: Vec<String>,
    ctl: HashSet<u8>,
    sgr: HashSet<String>,
    text: String,
    invalid_utf8: bool,
}

impl Facts {
    fn new(bytes: &[u8]) -> Self {
        let (toks, _) = scan(bytes);
        let mut csi = Vec::new();
        let mut esc = HashSet::new();
        let mut osc = Vec::new();
        let mut ctl = HashSet::new();
        let mut sgr = HashSet::new();
        let mut text = Vec::new();
        let mut invalid_utf8 = false;
        for tok in &toks {
            match tok {
                Tok::Csi(body) => {
                    let c = csi_parts(body);
                    if c.fin == 'm' && c.private.is_empty() && c.inter.is_empty() {
                        sgr.extend(sgr_items(&c.params));
                    }
                    csi.push(c);
                }
                Tok::Esc(body) => {
                    esc.insert(body.clone());
                }
                Tok::Osc(body) => osc.push(body.clone()),
                Tok::Ctl(b) => {
                    ctl.insert(*b);
                }
                Tok::Text(t) => {
                    invalid_utf8 |= std::str::from_utf8(t).is_err();
                    text.extend_from_slice(t);
                }
                Tok::Str(_) => {}
            }
        }
        Self {
            toks,
            csi,
            esc,
            osc,
            ctl,
            sgr,
            text: lossy(&text),
            invalid_utf8,
        }
    }

    /// A CSI sequence with exactly this prefix, parameters, intermediates and final byte.
    fn has(&self, private: &str, params: &str, inter: &str, fin: char) -> bool {
        self.csi
            .iter()
            .any(|c| c.private == private && c.params == params && c.inter == inter && c.fin == fin)
    }

    /// A plain CSI sequence (no prefix, no intermediate) with this final byte.
    fn has_fin(&self, fin: char) -> bool {
        self.csi
            .iter()
            .any(|c| c.private.is_empty() && c.inter.is_empty() && c.fin == fin)
    }

    fn sgr_has(&self, item: &str) -> bool {
        self.sgr.contains(item)
    }

    fn sgr_starts(&self, start: &str) -> bool {
        self.sgr.iter().any(|s| s.starts_with(start))
    }

    fn mode(&self, mode: u16, on: bool) -> bool {
        self.has("?", &mode.to_string(), "", if on { 'h' } else { 'l' })
    }

    fn osc_starts(&self, start: &str) -> bool {
        self.osc.iter().any(|o| o.starts_with(start))
    }

    fn char_in(&self, range: std::ops::RangeInclusive<u32>) -> bool {
        self.text.chars().any(|c| range.contains(&u32::from(c)))
    }
}

// ---------------------------------------------------------------------------------------------
// Coverage

/// Walks the corpus in order and counts what depends on order.
struct Order {
    max_title_depth: u32,
    extra_title_pops: u32,
    max_kitty_depth: u32,
    unclosed_sync: bool,
    rep_after_text: bool,
    rep_after_other: bool,
}

fn order(toks: &[Tok]) -> Order {
    let mut o = Order {
        max_title_depth: 0,
        extra_title_pops: 0,
        max_kitty_depth: 0,
        unclosed_sync: false,
        rep_after_text: false,
        rep_after_other: false,
    };
    let (mut title, mut kitty, mut sync) = (0u32, 0u32, false);
    for (i, tok) in toks.iter().enumerate() {
        if matches!(tok, Tok::Esc(e) if e == "c") {
            // RIS empties the stacks.
            (title, kitty) = (0, 0);
            continue;
        }
        let Tok::Csi(body) = tok else { continue };
        let c = csi_parts(body);
        match (c.private.as_str(), c.inter.as_str(), c.fin) {
            ("", "", 't') if c.params.starts_with("22;") => {
                title += 1;
                o.max_title_depth = o.max_title_depth.max(title);
            }
            ("", "", 't') if c.params.starts_with("23;") => {
                if title == 0 {
                    o.extra_title_pops += 1;
                }
                title = title.saturating_sub(1);
            }
            (">", "", 'u') => {
                kitty += 1;
                o.max_kitty_depth = o.max_kitty_depth.max(kitty);
            }
            ("<", "", 'u') => {
                kitty = kitty.saturating_sub(c.params.parse().unwrap_or(1));
            }
            ("?", "", 'h') if c.params == "2026" => {
                o.unclosed_sync |= sync;
                sync = true;
            }
            ("?", "", 'l') if c.params == "2026" => sync = false,
            ("", "", 'b') => match i.checked_sub(1).map(|p| &toks[p]) {
                Some(Tok::Text(_)) => o.rep_after_text = true,
                Some(_) => o.rep_after_other = true,
                None => {}
            },
            _ => {}
        }
    }
    o
}

/// The names of the kinds that the corpus lacks.
fn missing(ops: &[Op]) -> Vec<String> {
    let f = Facts::new(&bytes_of(ops));
    let o = order(&f.toks);
    let mut lacks: Vec<String> = Vec::new();
    let mut want = |name: String, present: bool| {
        if !present {
            lacks.push(name);
        }
    };

    // SGR.
    for n in [
        1, 2, 3, 4, 5, 7, 8, 9, 21, 22, 23, 24, 25, 27, 28, 29, 53, 55,
    ] {
        want(format!("SGR {n}"), f.sgr_has(&n.to_string()));
    }
    for n in (30..=37).chain(40..=47).chain(90..=97).chain(100..=107) {
        want(format!("SGR {n}"), f.sgr_has(&n.to_string()));
    }
    for start in [
        "38;5;", "48;5;", "38;2;", "48;2;", "38:2:", "48:2:", "38:2::", "38:5:", "48:5:", "58;5;",
        "58;2;", "58:2:",
    ] {
        want(format!("SGR {start}"), f.sgr_starts(start));
    }
    for item in ["4:1", "4:2", "4:3", "4:4", "4:5", "59", "39", "49", "0"] {
        want(format!("SGR {item}"), f.sgr_has(item));
    }

    // Cursor motion.
    for fin in ['H', 'f', 'A', 'B', 'C', 'D', 'E', 'F', 'G', 'd', '`'] {
        want(format!("CSI {fin}"), f.has_fin(fin));
    }
    want("ESC 7".into(), f.esc.contains("7"));
    want("ESC 8".into(), f.esc.contains("8"));
    want("CSI s".into(), f.has("", "", "", 's'));
    want("CSI u".into(), f.has("", "", "", 'u'));
    want(
        "cursor target out of range".into(),
        f.csi.iter().any(|c| {
            c.fin == 'H'
                && c.params
                    .split(';')
                    .any(|p| p.parse::<u32>().is_ok_and(|n| n > u32::from(MAX_COLS)))
        }),
    );

    // Scrolling.
    want(
        "DECSTBM top;bottom".into(),
        f.csi.iter().any(|c| {
            c.fin == 'r'
                && c.private.is_empty()
                && c.params
                    .split_once(';')
                    .is_some_and(|(a, b)| a.parse::<u32>().is_ok() && b.parse::<u32>().is_ok())
        }),
    );
    want("DECSTBM reset".into(), f.has("", "", "", 'r'));
    for e in ["D", "E", "M"] {
        want(format!("ESC {e}"), f.esc.contains(e));
    }
    want("SU".into(), f.has_fin('S'));
    want("SD".into(), f.has_fin('T'));

    // Modes.
    for (mode, name) in [(6, "origin"), (7, "autowrap")] {
        want(format!("{name} on"), f.mode(mode, true));
        want(format!("{name} off"), f.mode(mode, false));
    }
    for (params, name) in [("4", "insert"), ("20", "LNM")] {
        want(format!("{name} on"), f.has("", params, "", 'h'));
        want(format!("{name} off"), f.has("", params, "", 'l'));
    }

    // Editing.
    for fin in ['@', 'P', 'X', 'K', 'L', 'M'] {
        want(format!("CSI {fin}"), f.has_fin(fin));
    }
    for n in 0..=3 {
        let plain = f.has("", &n.to_string(), "", 'J') || (n == 0 && f.has("", "", "", 'J'));
        want(format!("ED {n}"), plain);
    }
    for n in 0..=2 {
        let plain = f.has("", &n.to_string(), "", 'K') || (n == 0 && f.has("", "", "", 'K'));
        want(format!("EL {n}"), plain);
    }

    // Tabs.
    want("HT".into(), f.ctl.contains(&b'\t'));
    want("HTS".into(), f.esc.contains("H"));
    want("TBC 0".into(), f.has("", "0", "", 'g'));
    want("TBC 3".into(), f.has("", "3", "", 'g'));
    want("CHT".into(), f.has_fin('I'));
    want("CBT".into(), f.has_fin('Z'));

    // Charsets.
    for e in ["(0", "(B", ")0", "*0", "+0", "n", "o"] {
        want(format!("ESC {e}"), f.esc.contains(e));
    }
    want("SO".into(), f.ctl.contains(&0x0e));
    want("SI".into(), f.ctl.contains(&0x0f));

    // Screens.
    for m in [1049, 47, 1047] {
        want(format!("?{m}h"), f.mode(m, true));
        want(format!("?{m}l"), f.mode(m, false));
    }

    // Characters.
    want("CJK".into(), f.char_in(0x4E00..=0x9FFF));
    want("fullwidth".into(), f.char_in(0xFF01..=0xFF60));
    want("combining".into(), f.char_in(0x0300..=0x036F));
    want("emoji".into(), f.char_in(0x1F300..=0x1FAFF));
    want("ZWJ".into(), f.char_in(0x200D..=0x200D));
    want("regional indicator".into(), f.char_in(0x1F1E6..=0x1F1FF));
    want("invalid UTF-8".into(), f.invalid_utf8);
    want("BEL".into(), f.ctl.contains(&7));

    // Titles and the title stack.
    for ps in ["0;", "1;", "2;"] {
        want(format!("OSC {ps}"), f.osc_starts(ps));
    }
    want("push title".into(), f.has("", "22;0", "", 't'));
    want("pop title".into(), f.has("", "23;0", "", 't'));
    want("many title pushes".into(), o.max_title_depth >= 20);
    want("more pops than pushes".into(), o.extra_title_pops > 0);

    // Palette and colours.
    for ps in ["4;", "10;", "11;", "12;", "8;;", "52;"] {
        want(format!("OSC {ps}"), f.osc_starts(ps));
    }
    for ps in ["104", "110", "111", "112"] {
        want(
            format!("OSC {ps}"),
            f.osc
                .iter()
                .any(|o| o == ps || o.starts_with(&format!("{ps};"))),
        );
    }
    want(
        "OSC 8 open".into(),
        f.osc.iter().any(|o| {
            o.strip_prefix("8;")
                .and_then(|r| r.split_once(';'))
                .is_some_and(|(_, uri)| !uri.is_empty())
        }),
    );
    want("OSC 8 close".into(), f.osc.iter().any(|o| o == "8;;"));

    // Cursor style and visibility.
    for n in 0..=6 {
        want(format!("DECSCUSR {n}"), f.has("", &n.to_string(), " ", 'q'));
    }
    want("cursor shown".into(), f.mode(25, true));
    want("cursor hidden".into(), f.mode(25, false));

    // Mouse, keys, paste, focus.
    for m in [9, 1000, 1002, 1003, 1005, 1006, 1015, 1, 2004, 1004, 1007] {
        want(format!("?{m}h"), f.mode(m, true));
        want(format!("?{m}l"), f.mode(m, false));
    }
    want("ESC =".into(), f.esc.contains("="));
    want("ESC >".into(), f.esc.contains(">"));
    want(
        "kitty push".into(),
        f.csi.iter().any(|c| c.private == ">" && c.fin == 'u'),
    );
    want(
        "kitty pop".into(),
        f.csi.iter().any(|c| c.private == "<" && c.fin == 'u'),
    );
    want(
        "kitty set".into(),
        f.csi
            .iter()
            .any(|c| c.private == "=" && c.fin == 'u' && c.params.contains(';')),
    );
    want("many kitty pushes".into(), o.max_kitty_depth >= 20);
    want("kitty stack past 4096".into(), o.max_kitty_depth > 4096);
    want("title stack past 4096".into(), o.max_title_depth > 4096);
    want(
        "modifyOtherKeys".into(),
        f.csi
            .iter()
            .any(|c| c.private == ">" && c.fin == 'm' && c.params.starts_with("4;")),
    );

    // REP, resets, DECCOLM, synchronized updates.
    want("REP after text".into(), o.rep_after_text);
    want("REP after other".into(), o.rep_after_other);
    want("DECALN".into(), f.esc.contains("#8"));
    want("RIS".into(), f.esc.contains("c"));
    want("DECSTR".into(), f.has("", "", "!", 'p'));
    want("DECCOLM on".into(), f.mode(3, true));
    want("DECCOLM off".into(), f.mode(3, false));
    want("sync begin".into(), f.mode(2026, true));
    want("sync end".into(), f.mode(2026, false));
    want("sync not closed".into(), o.unclosed_sync);

    // Queries.
    want(
        "DA1".into(),
        f.has("", "", "", 'c') || f.has("", "0", "", 'c'),
    );
    want(
        "DA2".into(),
        f.csi.iter().any(|c| c.private == ">" && c.fin == 'c'),
    );
    want(
        "DA3".into(),
        f.csi.iter().any(|c| c.private == "=" && c.fin == 'c'),
    );
    want(
        "XTVERSION".into(),
        f.csi.iter().any(|c| c.private == ">" && c.fin == 'q'),
    );
    want("DSR 6".into(), f.has("", "6", "", 'n'));
    want("DSR 5".into(), f.has("", "5", "", 'n'));
    want(
        "DECRQM".into(),
        f.csi.iter().any(|c| c.inter == "$" && c.fin == 'p'),
    );
    lacks
}

/// The text of exactly one row, followed by a control character, by a cursor query, or by more
/// text. The text starts at the first column. Counted for the terminal size at that point.
fn pending_wrap_cases(ops: &[Op], mut cols: u16) -> (usize, usize, usize) {
    let mut segments: Vec<(u16, Vec<u8>)> = vec![(cols, Vec::new())];
    for op in ops {
        match op {
            Op::Bytes(b) => segments
                .last_mut()
                .expect("a segment")
                .1
                .extend_from_slice(b),
            Op::Resize { cols: c, .. } => {
                cols = *c;
                segments.push((cols, Vec::new()));
            }
        }
    }
    let (mut control, mut query, mut more) = (0, 0, 0);
    for (cols, bytes) in segments {
        let (toks, _) = scan(&bytes);
        for w in toks.windows(3) {
            let (Tok::Csi(cup), Tok::Text(text), next) = (&w[0], &w[1], &w[2]) else {
                continue;
            };
            let c = csi_parts(cup);
            let at_first_column = (c.fin == 'H' || c.fin == 'f')
                && c.params
                    .rsplit(';')
                    .next()
                    .is_none_or(|p| p == "1" || !c.params.contains(';'));
            let ascii = text.iter().all(|b| (0x20..0x7f).contains(b));
            if !at_first_column || !ascii || text.len() != usize::from(cols) {
                continue;
            }
            match next {
                Tok::Ctl(_) => control += 1,
                Tok::Csi(q) if csi_parts(q).fin == 'n' => query += 1,
                _ => more += 1,
            }
        }
    }
    (control, query, more)
}

// ---------------------------------------------------------------------------------------------
// Tests

#[test]
fn the_same_seed_gives_the_same_corpus() {
    for seed in [0, 1, 99, u64::MAX] {
        assert_eq!(corpus(seed, 150_000), corpus(seed, 150_000), "seed {seed}");
    }
    assert_ne!(corpus(1, 20_000), corpus(2, 20_000));
    assert_ne!(corpus(0, 20_000), corpus(u64::MAX, 20_000));
}

/// The corpus must not depend on the platform, the build or the run. If this test fails after a
/// change of the generator that you meant to make, take the new value from the message.
#[test]
fn the_corpus_has_a_known_hash() {
    fn fnv(hash: &mut u64, bytes: &[u8]) {
        for b in bytes {
            *hash = (*hash ^ u64::from(*b)).wrapping_mul(0x0000_0100_0000_01B3);
        }
    }
    let mut hash = 0xCBF2_9CE4_8422_2325u64;
    for op in corpus(42, 200_000) {
        match op {
            Op::Bytes(b) => {
                fnv(&mut hash, &[0]);
                fnv(&mut hash, &(b.len() as u64).to_le_bytes());
                fnv(&mut hash, &b);
            }
            Op::Resize { cols, rows } => {
                fnv(&mut hash, &[1]);
                fnv(&mut hash, &cols.to_le_bytes());
                fnv(&mut hash, &rows.to_le_bytes());
            }
        }
    }
    assert_eq!(hash, 0x21a3_a342_5fae_c689, "the hash is now {hash:#018x}");
}

#[test]
fn the_size_is_within_bounds() {
    for seed in [0, 5, 77] {
        for bytes in [0, 1, 2, 10, 100, 4096, 5000, 70_000, MIB] {
            let ops = corpus(seed, bytes);
            let n = total(&ops);
            assert!(
                (bytes..=bytes + 4096).contains(&n),
                "seed {seed}, {bytes} bytes: got {n}"
            );
        }
    }
    assert!(corpus(3, 0).is_empty());
}

#[test]
fn the_chunks_are_small_and_whole() {
    for seed in SEEDS {
        let ops = corpus(seed, MIB);
        let mut sizes = Vec::new();
        for (n, op) in ops.iter().enumerate() {
            let Op::Bytes(b) = op else { continue };
            assert!(
                (1..=MAX_CHUNK).contains(&b.len()),
                "seed {seed}, op {n}: {} bytes",
                b.len()
            );
            let (_, ground) = scan(b);
            assert!(
                ground,
                "seed {seed}, op {n}: a sequence is cut at the end of the chunk"
            );
            assert!(
                !ends_inside_a_character(b),
                "seed {seed}, op {n}: a character is cut at the end of the chunk"
            );
            sizes.push(b.len());
        }
        assert!(sizes.contains(&1), "seed {seed}: no chunk of 1 byte");
        assert!(
            sizes.iter().any(|&n| n > 2048),
            "seed {seed}: no chunk above 2 KiB"
        );
        assert!(
            sizes.len() > 200,
            "seed {seed}: only {} chunks",
            sizes.len()
        );
        // Scanning the chunks one by one and scanning the whole stream find the same sequences.
        let whole = scan(&bytes_of(&ops)).0;
        let separate: usize = ops
            .iter()
            .map(|op| match op {
                Op::Bytes(b) => scan(b)
                    .0
                    .iter()
                    .filter(|t| !matches!(t, Tok::Text(_)))
                    .count(),
                Op::Resize { .. } => 0,
            })
            .sum();
        let in_whole = whole.iter().filter(|t| !matches!(t, Tok::Text(_))).count();
        assert_eq!(separate, in_whole, "seed {seed}");
    }
}

#[test]
fn resizes_are_in_range_and_not_too_many() {
    let mut shrinks = 0;
    let mut grows = 0;
    for seed in SEEDS {
        let ops = corpus(seed, MIB);
        let (mut cols, mut rows) = (80u16, 24u16);
        let mut resizes = 0;
        let mut since = 0usize;
        for op in &ops {
            match op {
                Op::Bytes(b) => since += b.len(),
                Op::Resize { cols: c, rows: r } => {
                    assert!(
                        (MIN_COLS..=MAX_COLS).contains(c) && (MIN_ROWS..=MAX_ROWS).contains(r),
                        "seed {seed}: resize to {c}x{r}"
                    );
                    assert_ne!(
                        (*c, *r),
                        (cols, rows),
                        "seed {seed}: resize to the same size"
                    );
                    shrinks += usize::from(*c < cols || *r < rows);
                    grows += usize::from(*c > cols || *r > rows);
                    (cols, rows) = (*c, *r);
                    resizes += 1;
                    // One resize for each 50 to 100 KB (the resize scene itself adds a little).
                    assert!(since >= 45_000, "seed {seed}: a resize after {since} bytes");
                    assert!(since < 180_000, "seed {seed}: no resize for {since} bytes");
                    since = 0;
                }
            }
        }
        assert!(
            (8..=25).contains(&resizes),
            "seed {seed}: {resizes} resizes"
        );
    }
    assert!(
        shrinks >= 10 && grows >= 10,
        "shrinks {shrinks}, grows {grows}"
    );
}

#[test]
fn resizes_happen_on_both_screens() {
    let (mut on_alt, mut on_primary) = (0, 0);
    for seed in SEEDS {
        let mut alt = false;
        for op in corpus(seed, MIB) {
            match op {
                Op::Bytes(b) => {
                    for tok in scan(&b).0 {
                        if let Tok::Csi(body) = tok {
                            match body.as_str() {
                                "?1049h" => alt = true,
                                "?1049l" => alt = false,
                                _ => {}
                            }
                        }
                    }
                }
                Op::Resize { .. } => {
                    if alt {
                        on_alt += 1;
                    } else {
                        on_primary += 1;
                    }
                }
            }
        }
    }
    assert!(
        on_alt >= 5 && on_primary >= 5,
        "alt {on_alt}, primary {on_primary}"
    );
}

#[test]
fn every_kind_of_state_is_covered_for_every_seed() {
    for seed in SEEDS {
        let lacks = missing(&corpus(seed, MIB));
        assert!(lacks.is_empty(), "seed {seed} lacks: {lacks:?}");
    }
}

/// A wider sweep for a change of the generator: 200 seeds of 1 MiB, about 30 s in a release
/// build. It runs only when `NEOHAR_SWEEP=1`.
#[test]
fn every_kind_of_state_is_covered_for_many_seeds() {
    if std::env::var_os("NEOHAR_SWEEP").is_none_or(|v| v != "1") {
        eprintln!("skipped: set NEOHAR_SWEEP=1 to run the sweep of 200 seeds");
        return;
    }
    for seed in 100..300 {
        let ops = corpus(seed, MIB);
        let lacks = missing(&ops);
        assert!(lacks.is_empty(), "seed {seed} lacks: {lacks:?}");
        let (control, query, _) = pending_wrap_cases(&ops, 80);
        assert!(
            control >= 1 && query >= 1,
            "seed {seed}: {control}, {query}"
        );
    }
}

#[test]
fn rows_are_filled_exactly_to_the_right_margin() {
    for seed in SEEDS {
        let ops = corpus(seed, MIB);
        let (control, query, _) = pending_wrap_cases(&ops, 80);
        assert!(
            control >= 5,
            "seed {seed}: {control} rows followed by a control"
        );
        assert!(
            query >= 3,
            "seed {seed}: {query} rows followed by a cursor query"
        );
    }
}

#[test]
fn the_initial_size_can_be_chosen() {
    let ops = corpus_from(11, 400_000, 132, 50);
    let (control, query, _) = pending_wrap_cases(&ops, 132);
    assert!(
        control >= 2 && query >= 1,
        "control {control}, query {query}"
    );
    assert!(total(&ops) >= 400_000);
    assert_ne!(ops, corpus(11, 400_000), "the size changes the corpus");
}

#[test]
fn resizes_stay_in_range_when_the_corpus_starts_outside_it() {
    for (cols, rows) in [(500, 100), (1, 1), (200, 60), (2, 1)] {
        let ops = corpus_from(4, 400_000, cols, rows);
        let resizes: Vec<_> = ops
            .iter()
            .filter_map(|op| match op {
                Op::Resize { cols, rows } => Some((*cols, *rows)),
                Op::Bytes(_) => None,
            })
            .collect();
        assert!(
            resizes.len() >= 3,
            "{cols}x{rows}: {} resizes",
            resizes.len()
        );
        for (c, r) in resizes {
            assert!(
                (MIN_COLS..=MAX_COLS).contains(&c) && (MIN_ROWS..=MAX_ROWS).contains(&r),
                "start {cols}x{rows}: resize to {c}x{r}"
            );
        }
    }
}
