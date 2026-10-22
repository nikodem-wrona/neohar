//! A synthetic terminal-output corpus, reproducible from a seed.
//!
//! The corpus feeds the snapshot and restore check of a VT engine, so its job is to put every
//! piece of terminal state into a non-default value: cursor, pending wrap, margins, modes, tab
//! stops, charsets, titles, palette, hyperlinks, keyboard stacks, both screens. It is a weighted
//! mix of *scenes*, each of a few hundred bytes. A scene is a small program (a shell prompt, a
//! full-screen application, a wide-character test, ...), so that state builds up the way it does
//! under real programs. A *salad* scene draws sequences from a shuffled deck that holds one entry
//! for every kind of sequence that the corpus must cover, so each kind appears at least once per
//! pass through the deck, whatever the seed. (The sequences that wipe the state, RIS, soft reset
//! and DECCOLM, are in a second deck that the salad uses a few times in 1 MiB.) Values are valid
//! most of the time and out of range now and then, on purpose.
//!
//! The output is a list of [`Op`]s. Every `Op::Bytes` chunk holds 1 B to 4 KiB and starts and ends
//! at the ground state of the parser: the generator emits *atoms* (one complete escape sequence,
//! one control character, one character or one grapheme cluster) and cuts the stream only between
//! atoms. Some atoms hold deliberately invalid bytes; those are also self-contained. The only
//! thing that can stay open across chunks is a synchronized update (`CSI ? 2026 h`), and that is
//! on purpose.
//!
//! Now and then (about once for each 300 KB) a scene of about 30 KB pushes the kitty keyboard stack
//! or the title stack past its limit of 4096 entries.
//!
//! The generator uses no source of randomness except a splitmix64 generator inside this file, no
//! hash map and no clock, so a `(seed, bytes)` pair gives the same list on every platform.

use std::fmt::Display;

use crate::Op;

/// Smallest column count of a resize in a corpus.
pub const MIN_COLS: u16 = 2;
/// Largest column count of a resize in a corpus.
pub const MAX_COLS: u16 = 200;
/// Smallest row count of a resize in a corpus.
pub const MIN_ROWS: u16 = 1;
/// Largest row count of a resize in a corpus.
pub const MAX_ROWS: u16 = 60;
/// The largest `Op::Bytes` chunk.
pub const MAX_CHUNK: usize = 4096;

/// A reproducible stream of operations. The `Op::Bytes` chunks hold at least `bytes` bytes and at
/// most `bytes` plus 4 KiB (the generator stops at the first complete sequence after `bytes`).
/// `Op::Resize` operations are mixed in, about one for each 50 to 100 KB of output. The terminal
/// starts at 80 columns and 24 rows.
pub fn corpus(seed: u64, bytes: usize) -> Vec<Op> {
    corpus_from(seed, bytes, 80, 24)
}

/// Like [`corpus`], for a terminal that starts at `cols` x `rows`.
pub fn corpus_from(seed: u64, bytes: usize, cols: u16, rows: u16) -> Vec<Op> {
    let mut g = Gen::new(seed, bytes, cols, rows);
    let mut next_resize = g.rng.range(50_000, 100_000) as usize;
    let mut next_deep = g.rng.range(150_000, 300_000) as usize;
    let mut deeps = 0;
    while !g.full() {
        let before = g.total;
        if g.total >= next_resize {
            resize_scene(&mut g);
            next_resize = g.total + g.rng.range(50_000, 100_000) as usize;
        } else if g.total >= next_deep {
            deep_stack(&mut g, deeps);
            deeps += 1;
            next_deep = g.total + g.rng.range(200_000, 350_000) as usize;
        } else {
            let scene = g.scene();
            scene(&mut g);
        }
        if g.total == before {
            // A scene always writes. This keeps the loop finite if one ever does not.
            g.text("x");
        }
    }
    g.finish()
}

// ---------------------------------------------------------------------------------------------
// Random numbers

/// splitmix64.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number in `0..n`, or 0 when `n` is 0.
    fn below(&mut self, n: u32) -> u32 {
        ((u128::from(self.next()) * u128::from(n)) >> 64) as u32
    }

    /// An index in `0..len`, or 0 when `len` is 0.
    fn index(&mut self, len: usize) -> usize {
        ((u128::from(self.next()) * len as u128) >> 64) as usize
    }

    /// A number in `lo..=hi`; `lo` when `hi <= lo`.
    fn range(&mut self, lo: u32, hi: u32) -> u32 {
        if hi <= lo {
            lo
        } else {
            lo + self.below(hi - lo + 1)
        }
    }

    fn chance(&mut self, num: u32, den: u32) -> bool {
        self.below(den) < num
    }

    /// An element of a table that is not empty.
    fn pick<T: Copy>(&mut self, items: &[T]) -> T {
        items[self.index(items.len())]
    }

    /// An index chosen by weight.
    fn weighted(&mut self, weights: impl Iterator<Item = u32> + Clone) -> usize {
        let total: u32 = weights.clone().sum();
        let mut roll = self.below(total);
        for (i, w) in weights.enumerate() {
            if roll < w {
                return i;
            }
            roll -= w;
        }
        0
    }

    fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.index(i + 1);
            items.swap(i, j);
        }
    }
}

fn chunk_target(rng: &mut Rng) -> usize {
    match rng.below(10) {
        0 => 1,
        1..=4 => rng.range(2, 64) as usize,
        5..=7 => rng.range(65, 1024) as usize,
        _ => rng.range(1025, MAX_CHUNK as u32) as usize,
    }
}

// ---------------------------------------------------------------------------------------------
// The generator

/// One entry of the deck of the salad scene.
#[derive(Clone, Copy)]
enum Item {
    /// A fixed sequence.
    Raw(&'static str),
    /// `CSI ? mode h` or `CSI ? mode l`.
    Mode(u16, bool),
    /// A sequence with random values.
    Call(fn(&mut Gen, u16), u16),
}

struct Gen {
    rng: Rng,
    ops: Vec<Op>,
    /// The chunk in the making.
    cur: Vec<u8>,
    /// The size that the next chunk should not exceed (unless one atom is larger).
    target: usize,
    /// Bytes in `ops` and `cur`.
    total: usize,
    limit: usize,
    /// The size that the terminal has now, as the corpus knows it.
    cols: u16,
    rows: u16,
    /// Whether the last `?1049` was a set. The engine ignores `?47` and `?1047`.
    alt: bool,
    deck: Deck,
    /// RIS, soft reset and DECCOLM. They wipe state, so the salad draws them seldom.
    rare: Deck,
    salads: u32,
}

/// Cards that are drawn in a shuffled order: every card comes up once for each pass.
struct Deck {
    items: Vec<Item>,
    order: Vec<usize>,
    next: usize,
}

impl Deck {
    fn new(items: Vec<Item>) -> Self {
        let order: Vec<usize> = (0..items.len()).collect();
        Self {
            next: order.len(),
            items,
            order,
        }
    }

    fn draw(&mut self, rng: &mut Rng) -> Item {
        if self.next >= self.order.len() {
            rng.shuffle(&mut self.order);
            self.next = 0;
        }
        let item = self.items[self.order[self.next]];
        self.next += 1;
        item
    }
}

impl Gen {
    fn new(seed: u64, limit: usize, cols: u16, rows: u16) -> Self {
        let mut rng = Rng(seed);
        let target = chunk_target(&mut rng);
        Self {
            rng,
            ops: Vec::new(),
            cur: Vec::new(),
            target,
            total: 0,
            limit,
            cols: cols.max(1),
            rows: rows.max(1),
            alt: false,
            deck: Deck::new(deck()),
            rare: Deck::new(rare_deck()),
            salads: 0,
        }
    }

    fn full(&self) -> bool {
        self.total >= self.limit
    }

    fn finish(mut self) -> Vec<Op> {
        self.flush();
        self.ops
    }

    fn flush(&mut self) {
        if !self.cur.is_empty() {
            self.ops.push(Op::Bytes(std::mem::take(&mut self.cur)));
        }
        self.target = chunk_target(&mut self.rng);
    }

    /// Adds one atom: a unit that the stream can be cut before and after. Nothing is added once
    /// the corpus has enough bytes.
    fn atom(&mut self, bytes: &[u8]) {
        if self.full() || bytes.is_empty() {
            return;
        }
        debug_assert!(bytes.len() <= MAX_CHUNK, "an atom is larger than a chunk");
        if !self.cur.is_empty() && self.cur.len() + bytes.len() > self.target.min(MAX_CHUNK) {
            self.flush();
        }
        self.cur.extend_from_slice(bytes);
        self.total += bytes.len();
    }

    fn resize(&mut self, cols: u16, rows: u16) {
        if self.full() {
            return;
        }
        self.flush();
        self.ops.push(Op::Resize { cols, rows });
        self.cols = cols;
        self.rows = rows;
    }

    fn scene(&mut self) -> fn(&mut Gen) {
        let i = self.rng.weighted(SCENES.iter().map(|s| s.0));
        SCENES[i].1
    }

    // --- Pieces ---

    /// A sequence or a control character, as one atom.
    fn s(&mut self, text: &str) {
        self.atom(text.as_bytes());
    }

    /// Text, as one atom for each character.
    fn text(&mut self, text: &str) {
        let mut buf = [0u8; 4];
        for c in text.chars() {
            self.atom(c.encode_utf8(&mut buf).as_bytes());
        }
    }

    fn cluster_from(&mut self, table: &[&'static str]) {
        let c = self.rng.pick(table);
        self.cluster(c);
    }

    /// A grapheme cluster. It is one atom, or, now and then, one atom for each character.
    fn cluster(&mut self, text: &str) {
        if self.chance(1, 5) {
            self.text(text);
        } else {
            self.s(text);
        }
    }

    fn chance(&mut self, num: u32, den: u32) -> bool {
        self.rng.chance(num, den)
    }

    fn cols32(&self) -> u32 {
        u32::from(self.cols)
    }

    fn rows32(&self) -> u32 {
        u32::from(self.rows)
    }

    fn csi(&mut self, params: impl Display, fin: impl Display) {
        self.s(&format!("\x1b[{params}{fin}"));
    }

    fn sgr(&mut self, params: impl Display) {
        self.csi(params, 'm');
    }

    /// An OSC sequence; the terminator is BEL or ST.
    fn osc(&mut self, body: &str) {
        let end = if self.chance(1, 3) { "\x1b\\" } else { "\x07" };
        self.s(&format!("\x1b]{body}{end}"));
    }

    fn mode(&mut self, mode: u16, on: bool) {
        if mode == 1049 {
            self.alt = on;
        }
        self.csi(format!("?{mode}"), if on { 'h' } else { 'l' });
    }

    fn byte(&mut self) -> u32 {
        self.rng.below(256)
    }

    fn word(&mut self) -> &'static str {
        self.rng.pick(WORDS)
    }

    fn words(&mut self, n: u32) -> String {
        let mut s = String::new();
        for i in 0..n {
            if i > 0 {
                s.push(' ');
            }
            s.push_str(self.word());
        }
        s
    }

    /// ASCII text of exactly `n` characters.
    fn fill(&mut self, n: usize) -> String {
        let mut s = String::with_capacity(n + 12);
        while s.len() < n {
            s.push_str(self.word());
            s.push(' ');
        }
        s.truncate(n);
        s
    }

    fn noise(&mut self, n: u32) -> String {
        (0..n)
            .map(|_| char::from(b'!' + self.rng.below(94) as u8))
            .collect()
    }

    /// A row. Mostly inside the terminal; now and then 0 or beyond the last row.
    fn row(&mut self) -> u32 {
        let rows = self.rows32();
        match self.rng.below(40) {
            0 => 0,
            1 => rows + 1,
            2 => rows + self.rng.range(2, 40),
            3 => self.rng.pick(&[255, 999, 32_767, 65_535, 65_536, 100_000]),
            _ => self.rng.range(1, rows),
        }
    }

    /// A column. Mostly inside the terminal; now and then 0 or beyond the last column.
    fn col(&mut self) -> u32 {
        let cols = self.cols32();
        match self.rng.below(40) {
            0 => 0,
            1 => cols + 1,
            2 => cols + self.rng.range(2, 300),
            3 => self.rng.pick(&[255, 999, 32_767, 65_535, 65_536, 100_000]),
            _ => self.rng.range(1, cols),
        }
    }

    /// A row inside the terminal.
    fn row_in(&mut self) -> u32 {
        let rows = self.rows32();
        self.rng.range(1, rows)
    }

    /// A column inside the terminal.
    fn col_in(&mut self) -> u32 {
        let cols = self.cols32();
        self.rng.range(1, cols)
    }

    /// A count for a relative movement or an edit, up to about `max`.
    fn count(&mut self, max: u32) -> u32 {
        match self.rng.below(20) {
            0..=11 => self.rng.range(1, 4),
            12..=16 => self.rng.range(1, max),
            17 => 0,
            18 => max + self.rng.range(1, 50),
            _ => self.rng.pick(&[1000, 65_535, 70_000]),
        }
    }

    /// The text of a count; an empty parameter for 1 now and then.
    fn count_param(&mut self, n: u32) -> String {
        if n == 1 && self.chance(1, 3) {
            String::new()
        } else {
            n.to_string()
        }
    }

    fn cup(&mut self, row: u32, col: u32) {
        let fin = if self.chance(1, 8) { 'f' } else { 'H' };
        let params = match (row, col, self.chance(1, 4)) {
            (1, 1, true) => String::new(),
            (_, 1, true) => row.to_string(),
            (1, _, true) => format!(";{col}"),
            _ => format!("{row};{col}"),
        };
        self.csi(params, fin);
    }

    fn newline(&mut self) {
        let nl = match self.rng.below(20) {
            0..=13 => "\r\n",
            14 => "\n",
            15 => "\x0b",
            16 => "\x0c",
            17 => "\x1bE",
            18 => "\x1bD",
            _ => "\n\r",
        };
        self.s(nl);
    }

    fn color_spec(&mut self) -> String {
        let (r, gr, b) = (self.byte(), self.byte(), self.byte());
        match self.rng.below(12) {
            0..=3 => format!("rgb:{r:02x}/{gr:02x}/{b:02x}"),
            4 => format!("rgb:{:x}/{:x}/{:x}", r >> 4, gr >> 4, b >> 4),
            5 => format!("rgb:{r:02x}{r:02x}/{gr:02x}{gr:02x}/{b:02x}{b:02x}"),
            6 | 7 => format!("#{r:02x}{gr:02x}{b:02x}"),
            8 => format!("#{:x}{:x}{:x}", r >> 4, gr >> 4, b >> 4),
            9 => "?".to_owned(),
            _ => self
                .rng
                .pick(&[
                    "red",
                    "rgb:zz/00/00",
                    "#12",
                    "rgb:1/2",
                    "",
                    "rgb:ff/ff/ff/ff",
                ])
                .to_owned(),
        }
    }

    fn base64(&mut self, n: usize) -> String {
        const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let data: Vec<u8> = (0..n).map(|_| self.rng.below(256) as u8).collect();
        let mut out = String::new();
        for group in data.chunks(3) {
            let b = [
                group.first().copied().unwrap_or(0),
                group.get(1).copied().unwrap_or(0),
                group.get(2).copied().unwrap_or(0),
            ];
            let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
            for k in 0..4 {
                if k <= group.len() {
                    out.push(char::from(ABC[((n >> (18 - 6 * k)) & 63) as usize]));
                } else {
                    out.push('=');
                }
            }
        }
        out
    }

    fn title_text(&mut self) -> String {
        match self.rng.below(8) {
            0 => String::new(),
            1 => "日本語 ✓ ünï".to_owned(),
            2 => {
                let n = self.rng.range(120, 180) as usize;
                self.fill(n)
            }
            3 => "a;b;c".to_owned(),
            _ => {
                let n = self.rng.range(1, 4);
                self.words(n)
            }
        }
    }

    // --- Items of the deck: one sequence each, with random values ---

    fn sgr_one(&mut self, n: u16) {
        self.sgr(n);
    }

    /// `38`, `48` or `58` with a colour: `which * 10 + form`.
    fn sgr_ext(&mut self, code: u16) {
        let which = code / 10;
        let (n, r, gr, b) = (self.byte(), self.byte(), self.byte(), self.byte());
        let params = match code % 10 {
            0 => format!("{which};5;{n}"),
            1 => format!("{which};2;{r};{gr};{b}"),
            2 => format!("{which}:5:{n}"),
            3 => format!("{which}:2::{r}:{gr}:{b}"),
            _ => format!("{which}:2:{r}:{gr}:{b}"),
        };
        self.sgr(params);
    }

    fn sgr_item(&mut self) -> String {
        const ATTRS: [u16; 18] = [
            1, 2, 3, 4, 5, 7, 8, 9, 21, 22, 23, 24, 25, 27, 28, 29, 53, 55,
        ];
        let (n, r, gr, b) = (self.byte(), self.byte(), self.byte(), self.byte());
        match self.rng.below(100) {
            0..=29 => self.rng.pick(&ATTRS).to_string(),
            30..=39 => (30 + self.rng.below(8)).to_string(),
            40..=49 => (40 + self.rng.below(8)).to_string(),
            50..=54 => (90 + self.rng.below(8)).to_string(),
            55..=59 => (100 + self.rng.below(8)).to_string(),
            60..=69 => format!("{};5;{n}", self.rng.pick(&[38, 48])),
            70..=77 => format!("{};2;{r};{gr};{b}", self.rng.pick(&[38, 48])),
            78..=82 => match self.rng.below(4) {
                0 => format!("38:2::{r}:{gr}:{b}"),
                1 => format!("48:2:{r}:{gr}:{b}"),
                2 => format!("38:5:{n}"),
                _ => format!("48:2::{r}:{gr}:{b}"),
            },
            83..=86 => format!("4:{}", self.rng.below(6)),
            87..=90 => match self.rng.below(3) {
                0 => format!("58;5;{n}"),
                1 => format!("58;2;{r};{gr};{b}"),
                _ => format!("58:2::{r}:{gr}:{b}"),
            },
            91..=94 => self.rng.pick(&["0", "39", "49", "59", ""]).to_owned(),
            95..=96 => self
                .rng
                .pick(&[
                    "38;5;999",
                    "38;2;300;0;0",
                    "4:9",
                    "99",
                    "10",
                    "73",
                    "48;5;256",
                    "6",
                ])
                .to_owned(),
            _ => self.rng.pick(&["38;5", "48;2;1", "58", "38"]).to_owned(),
        }
    }

    fn emit_cup(&mut self, _: u16) {
        let (r, c) = (self.row(), self.col());
        self.cup(r, c);
    }

    fn emit_hvp(&mut self, _: u16) {
        let (r, c) = (self.row(), self.col());
        self.csi(format!("{r};{c}"), 'f');
    }

    /// A movement or an edit with a count, by its final byte.
    fn rel(&mut self, fin: u16) {
        let fin = char::from(u8::try_from(fin).unwrap_or(b'A'));
        match fin {
            'd' => {
                let r = self.row();
                self.csi(r, fin);
            }
            'G' | '`' => {
                let c = self.col();
                self.csi(c, fin);
            }
            _ => {
                let max = match fin {
                    'A' | 'B' | 'E' | 'F' | 'e' | 'L' | 'M' | 'S' | 'T' => self.rows32(),
                    'I' | 'Z' => 12,
                    _ => self.cols32(),
                };
                let n = self.count(max);
                let params = self.count_param(n);
                self.csi(params, fin);
            }
        }
    }

    /// A parameter that is a number, or empty for 0 now and then.
    fn zero_param(&mut self, n: u16) -> String {
        if n == 0 && self.chance(1, 3) {
            String::new()
        } else {
            n.to_string()
        }
    }

    fn ed(&mut self, n: u16) {
        let params = self.zero_param(n);
        self.csi(params, 'J');
    }

    fn el(&mut self, n: u16) {
        let params = self.zero_param(n);
        self.csi(params, 'K');
    }

    fn decstbm(&mut self, _: u16) {
        let rows = self.rows32();
        match self.rng.below(10) {
            0 => {
                // Not a region: top below bottom, or beyond the screen.
                let (a, b) = (self.rng.range(1, rows + 5), self.rng.range(0, rows + 5));
                self.csi(format!("{a};{b}"), 'r');
            }
            1 => {
                let a = self.rng.range(1, rows);
                self.csi(a, 'r');
            }
            2 => {
                let b = self.rng.range(1, rows);
                self.csi(format!(";{b}"), 'r');
            }
            _ => {
                let top = self.rng.range(1, rows.saturating_sub(1).max(1));
                let bottom = self.rng.range(top + 1, rows);
                self.csi(format!("{top};{bottom}"), 'r');
            }
        }
    }

    fn decscusr(&mut self, n: u16) {
        self.csi(format!("{n} "), 'q');
    }

    fn rep(&mut self, variant: u16) {
        let n = match self.rng.below(10) {
            0 => 0,
            1 => self.rng.range(100, 2000),
            _ => self.rng.range(1, 12),
        };
        match variant {
            0 => {
                let k = self.rng.range(1, 6);
                let t = self.noise(k);
                self.text(&t);
            }
            1 => match self.rng.below(5) {
                0 => self.s("\n"),
                1 => self.sgr(31),
                2 => self.s("\r"),
                3 => {
                    let (r, c) = (self.row_in(), self.col_in());
                    self.cup(r, c);
                }
                _ => self.s("\x1b7"),
            },
            2 => self.cluster("日"),
            _ => {
                let (r, c) = (self.row_in(), self.cols32());
                self.cup(r, c);
                self.text("Q");
            }
        }
        self.csi(if n == 1 { String::new() } else { n.to_string() }, 'b');
    }

    fn kitty_push(&mut self, _: u16) {
        let flags = if self.chance(1, 10) {
            63
        } else {
            self.rng.below(32)
        };
        let params = if flags == 0 && self.chance(1, 2) {
            String::new()
        } else {
            flags.to_string()
        };
        self.csi(format!(">{params}"), 'u');
    }

    fn kitty_pop(&mut self, _: u16) {
        let n = match self.rng.below(10) {
            0 => String::new(),
            1 => self.rng.range(15, 100).to_string(),
            2 => "0".to_owned(),
            3..=5 => "1".to_owned(),
            _ => self.rng.range(2, 4).to_string(),
        };
        self.csi(format!("<{n}"), 'u');
    }

    fn kitty_set(&mut self, _: u16) {
        let flags = self.rng.below(32);
        let params = match self.rng.below(5) {
            0 => format!("{flags}"),
            _ => format!("{flags};{}", self.rng.range(1, 3)),
        };
        self.csi(format!("={params}"), 'u');
    }

    fn modify_other_keys(&mut self, _: u16) {
        let n = self.rng.below(3);
        self.csi(format!(">4;{n}"), 'm');
    }

    /// RIS, and then a pop of each stack: both stacks are empty now, so these are pops on an empty
    /// stack.
    fn ris(&mut self, _: u16) {
        self.s("\x1bc");
        self.s("\x1b[23;0t");
        if self.chance(1, 2) {
            self.s("\x1b[<u");
        }
    }

    fn osc_title(&mut self, ps: u16) {
        let t = self.title_text();
        self.osc(&format!("{ps};{t}"));
    }

    fn osc4(&mut self, _: u16) {
        let pairs = self.rng.range(1, 3);
        let mut body = String::from("4");
        for _ in 0..pairs {
            let n = if self.chance(1, 30) {
                self.rng.range(256, 400)
            } else {
                self.rng.below(256)
            };
            let spec = self.color_spec();
            body.push_str(&format!(";{n};{spec}"));
        }
        self.osc(&body);
    }

    fn osc_color(&mut self, ps: u16) {
        let spec = self.color_spec();
        self.osc(&format!("{ps};{spec}"));
    }

    fn osc_reset(&mut self, ps: u16) {
        let body = match (ps, self.rng.below(4)) {
            (104, 1) => format!("104;{}", self.rng.below(256)),
            (104, 2) => format!("104;{};{}", self.rng.below(256), self.rng.below(256)),
            _ => ps.to_string(),
        };
        self.osc(&body);
    }

    fn url(&mut self) -> String {
        match self.rng.below(6) {
            0 => "http://example.com/a".to_owned(),
            1 => "https://example.org/path?q=1&r=2#frag".to_owned(),
            2 => "file:///tmp/some%20file.txt".to_owned(),
            3 => "mailto:someone@example.com".to_owned(),
            4 => {
                let n = self.rng.range(100, 180) as usize;
                format!("https://example.net/{}", self.fill(n).replace(' ', "-"))
            }
            _ => format!("http://example.com/{}", self.word()),
        }
    }

    /// A hyperlink with its text, closed (or, now and then, not).
    fn hyperlink(&mut self, _: u16) {
        let url = self.url();
        let params = match self.rng.below(4) {
            0 => format!("id={}", self.word()),
            1 => "id=x:y".to_owned(),
            _ => String::new(),
        };
        self.osc(&format!("8;{params};{url}"));
        let n = self.rng.range(1, 3);
        let text = self.words(n);
        self.text(&text);
        if !self.chance(1, 8) {
            self.osc("8;;");
        }
    }

    fn clipboard(&mut self, _: u16) {
        let target = self.rng.pick(&["c", "p", "", "s", "cp", "0"]);
        let body = match self.rng.below(8) {
            0 => "?".to_owned(),
            1 => String::new(),
            2 => "!!!not base64".to_owned(),
            _ => {
                let n = self.rng.range(1, 120) as usize;
                self.base64(n)
            }
        };
        self.osc(&format!("52;{target};{body}"));
    }

    fn notify(&mut self, k: u16) {
        let title = self.word();
        let body = self.words(2);
        if k == 0 {
            self.osc(&format!("9;{body}"));
        } else {
            self.osc(&format!("777;notify;{title};{body}"));
        }
    }

    fn words_item(&mut self, n: u16) {
        let k = self.rng.range(1, 2 + u32::from(n));
        let t = self.words(k);
        self.text(&t);
    }

    fn stray(&mut self, _: u16) {
        invalid_atom(self);
    }

    /// One piece of text from the wide-character tables: `0` CJK, `1` fullwidth, `2` combining
    /// marks, `3` emoji, `4` ZWJ sequences, `5` regional indicators.
    fn unicode_item(&mut self, kind: u16) {
        let table = match kind {
            0 => CJK,
            1 => FULLWIDTH,
            2 => COMBINING,
            3 => EMOJI,
            4 => ZWJ,
            _ => FLAGS,
        };
        let c = self.rng.pick(table);
        self.cluster(c);
    }

    fn unicode_wide(&mut self) {
        let kind = self.rng.pick(&[0, 0, 1, 3, 3, 4, 5]);
        self.unicode_item(kind);
    }

    fn unicode_any(&mut self) {
        let kind = self.rng.below(6) as u16;
        self.unicode_item(kind);
    }

    fn draw(&mut self) -> Item {
        self.deck.draw(&mut self.rng)
    }

    fn emit(&mut self, item: Item) {
        match item {
            Item::Raw(s) => self.s(s),
            Item::Mode(m, on) => self.mode(m, on),
            Item::Call(f, arg) => f(self, arg),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Tables

const WORDS: &[&str] = &[
    "the",
    "quick",
    "brown",
    "fox",
    "jumps",
    "over",
    "lazy",
    "dog",
    "cargo",
    "build",
    "error",
    "warning",
    "src",
    "main.rs",
    "Compiling",
    "Finished",
    "README.md",
    "target",
    "debug",
    "release",
    "0x7ffe",
    "42",
    "3.14159",
    "git",
    "commit",
    "HEAD",
    "origin/main",
    "ls",
    "-la",
    "grep",
    "fn",
    "let",
    "mut",
    "impl",
    "struct",
    "test",
    "ok",
    "FAILED",
    "passed",
    "/usr/local/bin",
    "~/src",
    "user@host",
    "{}",
    "[1/3]",
    "(done)",
    "a_b_c",
    "x=1",
    "--help",
    "2>&1",
];

const CJK: &[&str] = &[
    "日本語",
    "漢字",
    "テキスト",
    "한국어",
    "中文字符",
    "全角",
    "ひらがな",
    "字",
    "日",
];
const FULLWIDTH: &[&str] = &["Ａｂｃ１２３", "！？", "（）", "＠", "ＡＢＣ"];
const COMBINING: &[&str] = &[
    "e\u{301}",
    "a\u{300}\u{301}",
    "o\u{302}\u{323}",
    "n\u{303}",
    "ア\u{3099}",
    "ก\u{e49}",
    "x\u{20dd}",
    "\u{301}",
    "\u{302}\u{303}\u{304}",
    "\u{200b}",
    "\u{200d}",
    "\u{fe0f}",
    "\u{ad}",
];
const EMOJI: &[&str] = &[
    "😀",
    "🎉",
    "🚀",
    "👍🏽",
    "❤\u{fe0f}",
    "☺\u{fe0e}",
    "🧑🏿",
    "✂\u{fe0f}",
    "🦀",
];
const ZWJ: &[&str] = &[
    "👨\u{200d}👩\u{200d}👧\u{200d}👦",
    "🏳\u{fe0f}\u{200d}🌈",
    "🧑\u{200d}💻",
    "1\u{fe0f}\u{20e3}",
    "👩🏽\u{200d}🚀",
];
const FLAGS: &[&str] = &[
    "\u{1f1e9}\u{1f1ea}",
    "\u{1f1ef}\u{1f1f5}",
    "\u{1f1fa}\u{1f1f8}",
    "\u{1f1eb}\u{1f1f7}\u{1f1ea}\u{1f1f8}",
    "\u{1f1e9}",
];

/// Single wide characters (two cells each).
const WIDE_ONE: &[&str] = &["日", "本", "語", "字", "Ａ", "😀", "한"];

/// Modes of `CSI ? n h` and `CSI ? n l`. The first group is the one that the engine acts on.
const MODES: &[u16] = &[
    1, 6, 7, 12, 25, 1000, 1002, 1003, 1004, 1005, 1006, 1007, 1042, 2004, 2026, // known
    9, 47, 66, 1015, 1047, 1049, // mouse X10, alternate screens, keypad, urxvt mouse
    5, 8, 45, 69, 80, 1001, 1016, 1048, 2031, // not implemented, ignored
];

/// Modes that the mode scene toggles (no screen switch, no DECCOLM, no synchronized update).
const TOGGLES: &[u16] = &[
    1, 6, 7, 9, 12, 25, 66, 1000, 1002, 1003, 1004, 1005, 1006, 1007, 1015, 1042, 2004, 5, 8, 45,
    69, 80, 1001, 1016, 1048,
];

const QUERIES: &[&str] = &[
    "\x1b[c",
    "\x1b[0c",
    "\x1b[>c",
    "\x1b[>0c",
    "\x1b[=c",
    "\x1b[>q",
    "\x1b[>0q",
    "\x1b[6n",
    "\x1b[5n",
    "\x1b[?6n",
    "\x1b[?15n",
    "\x1b[?996n",
    "\x1b[?2004$p",
    "\x1b[?1049$p",
    "\x1b[?7$p",
    "\x1b[?25$p",
    "\x1b[?9999$p",
    "\x1b[4$p",
    "\x1b[20$p",
    "\x1b[14t",
    "\x1b[16t",
    "\x1b[18t",
    "\x1b[19t",
    "\x1b[21t",
    "\x1b]10;?\x07",
    "\x1b]11;?\x1b\\",
    "\x1b]12;?\x07",
    "\x1b]4;1;?\x07",
    "\x1b[?u",
    "\x1bZ",
    "\x1bP$qm\x1b\\",
    "\x1bP$qr\x1b\\",
    "\x1bP+q544e\x1b\\",
];

/// Sequences of the deck that do not need random values.
const FIXED: &[&str] = &[
    // Cursor save and restore, scrolling, tab stops, keypad.
    "\x1b7",
    "\x1b8",
    "\x1b[s",
    "\x1b[u",
    "\x1bD",
    "\x1bE",
    "\x1bM",
    "\x1bH",
    "\x1b=",
    "\x1b>",
    // Charsets.
    "\x1b(0",
    "\x1b(B",
    "\x1b)0",
    "\x1b)B",
    "\x1b*0",
    "\x1b+0",
    "\x1b(A",
    "\x1bn",
    "\x1bo",
    "\x0e",
    "\x0f",
    // Controls.
    "\x07",
    "\x08",
    "\t",
    "\r",
    "\n",
    "\x0b",
    "\x0c",
    // Tab clear, insert mode, LNM, resets, DECALN.
    "\x1b[0g",
    "\x1b[3g",
    "\x1b[g",
    "\x1b[4h",
    "\x1b[4l",
    "\x1b[20h",
    "\x1b[20l",
    "\x1b#8",
    "\x1b#3",
    "\x1b[r",
    // Title stack.
    "\x1b[22;0t",
    "\x1b[23;0t",
    "\x1b[22;1t",
    "\x1b[22;2t",
    "\x1b[23;1t",
    "\x1b[23;2t",
    // Kitty keyboard and modifyOtherKeys.
    "\x1b[<u",
    "\x1b[<1u",
    "\x1b[>4;2m",
    "\x1b[>4;1m",
    "\x1b[>4;0m",
    "\x1b[>4m",
    // Underline kinds.
    "\x1b[4:0m",
    "\x1b[4:1m",
    "\x1b[4:2m",
    "\x1b[4:3m",
    "\x1b[4:4m",
    "\x1b[4:5m",
    // Palette and dynamic colour resets, hyperlink close.
    "\x1b]104\x07",
    "\x1b]110\x07",
    "\x1b]111\x07",
    "\x1b]112\x1b\\",
    "\x1b]8;;\x1b\\",
    // Unknown things that the parser must skip.
    "\x1b[99z",
    "\x1b[?9999h",
    "\x1b[?9999l",
    "\x1b]999;x\x07",
    "\x1b%G",
    "\x1b_Ga=q;x\x1b\\",
    "\x1b^pm\x1b\\",
    "\x1bXsos\x1b\\",
];

fn deck() -> Vec<Item> {
    use Item::{Call, Mode, Raw};
    let mut v: Vec<Item> = FIXED.iter().map(|s| Raw(s)).collect();
    v.extend(QUERIES.iter().map(|s| Raw(s)));
    for n in [
        0, 1, 2, 3, 4, 5, 7, 8, 9, 21, 22, 23, 24, 25, 27, 28, 29, 39, 49, 53, 55, 59,
    ] {
        v.push(Call(Gen::sgr_one, n));
    }
    for n in (30..=37).chain(40..=47).chain(90..=97).chain(100..=107) {
        v.push(Call(Gen::sgr_one, n));
    }
    for which in [38, 48, 58] {
        for form in 0..5 {
            v.push(Call(Gen::sgr_ext, which * 10 + form));
        }
    }
    v.push(Call(Gen::emit_cup, 0));
    v.push(Call(Gen::emit_hvp, 0));
    for fin in "ABCDEFGd`aeIZ@PXLMST".chars() {
        v.push(Call(Gen::rel, fin as u16));
    }
    for n in 0..=3 {
        v.push(Call(Gen::ed, n));
    }
    for n in 0..=2 {
        v.push(Call(Gen::el, n));
    }
    v.push(Call(Gen::decstbm, 0));
    v.push(Call(Gen::decstbm, 0));
    for n in 0..=6 {
        v.push(Call(Gen::decscusr, n));
    }
    for &m in MODES {
        v.push(Mode(m, true));
        v.push(Mode(m, false));
    }
    for variant in 0..4 {
        v.push(Call(Gen::rep, variant));
    }
    v.push(Call(Gen::kitty_push, 0));
    v.push(Call(Gen::kitty_pop, 0));
    v.push(Call(Gen::kitty_set, 0));
    v.push(Call(Gen::modify_other_keys, 0));
    for ps in [0, 1, 2] {
        v.push(Call(Gen::osc_title, ps));
    }
    v.push(Call(Gen::osc4, 0));
    for ps in [10, 11, 12] {
        v.push(Call(Gen::osc_color, ps));
    }
    for ps in [104, 110, 111, 112] {
        v.push(Call(Gen::osc_reset, ps));
    }
    v.push(Call(Gen::hyperlink, 0));
    v.push(Call(Gen::clipboard, 0));
    v.push(Call(Gen::notify, 0));
    v.push(Call(Gen::notify, 1));
    for kind in 0..6 {
        v.push(Call(Gen::unicode_item, kind));
    }
    for n in 0..3 {
        v.push(Call(Gen::words_item, n));
    }
    v.push(Call(Gen::stray, 0));
    v
}

/// The sequences that wipe the state: RIS, soft reset and DECCOLM (which clears the screen).
fn rare_deck() -> Vec<Item> {
    use Item::{Call, Mode, Raw};
    vec![
        Call(Gen::ris, 0),
        Raw("\x1b[!p"),
        Mode(3, true),
        Mode(3, false),
    ]
}

type Scene = fn(&mut Gen);

/// The scenes and their weights.
const SCENES: &[(u32, Scene)] = &[
    (4, shell),
    (3, fullscreen),
    (2, unicode),
    (2, modes),
    (1, charset),
    (1, title),
    (1, palette),
    (4, salad),
    (2, pending_wrap),
    (2, scroll_region),
    (2, editing),
    (1, tabs),
    (2, keyboard_mouse),
    (1, links),
    (1, queries),
    (1, sync_update),
    (1, repeat),
    (1, invalid),
    (2, sgr_soup),
    (2, cursor_dance),
];

// ---------------------------------------------------------------------------------------------
// Scenes

/// A prompt, a command and its output.
fn shell(g: &mut Gen) {
    g.s("\r");
    if g.chance(1, 4) {
        g.osc("133;A");
    }
    if g.chance(1, 4) {
        g.osc("7;file://host/home/user/src");
    }
    if g.chance(1, 3) {
        let t = g.words(2);
        g.osc(&format!("0;{t}"));
    }
    if g.chance(1, 3) {
        g.mode(2004, true);
    }
    let colour = g.chance(2, 3);
    if colour {
        g.sgr("1;32");
    }
    g.text("user@host");
    if colour {
        g.sgr(0);
    }
    g.text(":");
    if colour {
        g.sgr("1;34");
    }
    g.text("~/src");
    if colour {
        g.sgr("0");
    }
    g.text("$ ");
    if g.chance(1, 4) {
        g.osc("133;B");
    }
    let n = g.rng.range(1, 4);
    let command = g.words(n);
    g.text(&command);
    if g.chance(1, 4) {
        // A correction: readline erases one character.
        match g.rng.below(2) {
            0 => g.s("\x08 \x08"),
            _ => {
                g.csi("", 'D');
                g.csi("", 'P');
            }
        }
        g.text("x");
    }
    if g.chance(1, 6) {
        g.s("\x07");
    }
    if g.chance(1, 3) {
        g.mode(2004, false);
    }
    g.newline();
    if g.chance(1, 5) {
        g.osc("133;C");
    }
    for _ in 0..g.rng.range(1, 5) {
        shell_line(g);
    }
    if g.chance(1, 5) {
        g.osc("133;D;0");
    }
}

fn shell_line(g: &mut Gen) {
    let cols = g.cols32();
    match g.rng.below(9) {
        0 => {
            let n = g.rng.range(1, 5);
            let line = g.words(n);
            g.text(&line);
        }
        1 => {
            // A line longer than the screen: it wraps.
            let n = g.rng.range(cols + 1, cols * 3).min(600) as usize;
            let line = g.fill(n);
            g.text(&line);
        }
        2 => {
            // `ls`: words and tabs.
            for _ in 0..g.rng.range(3, 8) {
                let w = g.word();
                g.text(w);
                g.s("\t");
            }
        }
        3 => {
            for _ in 0..g.rng.range(2, 5) {
                g.sgr("01;34");
                let w = g.word();
                g.text(w);
                g.sgr("0");
                g.text("  ");
            }
        }
        4 => {
            // A progress bar redrawn in place.
            for p in [10, 40, 70, 100] {
                g.text(&format!("\r[{:<8}] {p}%", "#".repeat(p / 13)));
            }
        }
        5 => {
            g.csi(2, 'K');
            g.s("\r");
            let n = g.rng.range(1, 4);
            let line = g.words(n);
            g.text(&line);
        }
        6 => {
            let n = g.rng.range(1, 4);
            let line = g.words(n);
            g.text(&line);
            g.csi("", 'K');
        }
        7 => {
            for _ in 0..3 {
                let w = g.word();
                g.text(w);
                g.s("\t");
            }
        }
        _ => {}
    }
    g.newline();
}

/// A full-screen application: alternate screen, margins, drawing at positions, scrolling.
fn fullscreen(g: &mut Gen) {
    let (cols, rows) = (g.cols32(), g.rows32());
    if !g.alt {
        match g.rng.below(10) {
            0 => g.mode(47, true),
            1 => g.mode(1047, true),
            _ => g.mode(1049, true),
        }
    }
    if g.chance(2, 3) {
        g.mode(25, false);
    }
    if g.chance(1, 3) {
        g.mode(1000, true);
        g.mode(1006, true);
    }
    if g.chance(1, 2) {
        g.csi(2, 'J');
        g.csi("", 'H');
    }
    let (top, bottom) = if rows >= 4 { (2, rows - 1) } else { (1, rows) };
    g.csi(format!("{top};{bottom}"), 'r');
    // The header, in reverse video.
    g.cup(1, 1);
    g.sgr(7);
    let header = g.fill(cols.min(40) as usize);
    g.text(&header);
    g.csi("", 'K');
    g.sgr(0);
    // The body.
    for _ in 0..g.rng.range(3, 9) {
        let r = g.rng.range(top, bottom);
        let c = g.col_in();
        g.cup(r, c);
        let style = g.sgr_item();
        g.sgr(style);
        let n = g.rng.range(1, 3);
        let line = g.words(n);
        g.text(&line);
        if g.chance(1, 2) {
            g.csi("", 'K');
        }
        g.sgr(0);
    }
    if g.chance(1, 3) {
        let (w, h) = (g.rng.range(2, 12), g.rng.range(1, 5));
        let r = g.rng.range(top, bottom.saturating_sub(h + 1).max(top));
        let c = g.rng.range(1, cols.saturating_sub(w + 2).max(1));
        acs_box(g, r, c, w, h);
    }
    // The status line: exactly one row of text, so that the cursor ends in the pending-wrap state.
    g.cup(rows, 1);
    g.sgr("30;47");
    let status = g.fill(cols as usize);
    g.text(&status);
    g.sgr(0);
    // Scrolling inside the region.
    match g.rng.below(4) {
        0 => {
            g.cup(bottom, 1);
            for _ in 0..g.rng.range(1, 4) {
                g.s("\n");
            }
        }
        1 => {
            g.cup(top, 1);
            for _ in 0..g.rng.range(1, 4) {
                g.s("\x1bM");
            }
        }
        2 => {
            let n = g.rng.range(1, 4);
            g.csi(n, 'S');
        }
        _ => {}
    }
    if g.chance(3, 4) {
        g.csi("", 'r');
        g.mode(1000, false);
        g.mode(1006, false);
        g.mode(25, true);
        match g.rng.below(14) {
            0 => g.mode(47, false),
            1 => g.mode(1047, false),
            _ => g.mode(1049, false),
        }
    }
}

fn acs_box(g: &mut Gen, row: u32, col: u32, w: u32, h: u32) {
    let inner = "q".repeat(w as usize);
    g.s("\x1b(0");
    g.cup(row, col);
    g.text(&format!("l{inner}k"));
    for i in 1..=h {
        g.cup(row + i, col);
        g.text("x");
        g.cup(row + i, col + w + 1);
        g.text("x");
    }
    g.cup(row + h + 1, col);
    g.text(&format!("m{inner}j"));
    g.s("\x1b(B");
}

/// Wide characters, combining marks and emoji, in the places where a cell grid breaks.
fn unicode(g: &mut Gen) {
    let cols = g.cols32();
    for _ in 0..g.rng.range(3, 8) {
        let (r, c) = (g.row_in(), g.col_in());
        match g.rng.below(11) {
            0 => {
                for _ in 0..g.rng.range(2, 6) {
                    if g.chance(1, 3) {
                        let w = g.word();
                        g.text(w);
                        g.text(" ");
                    } else {
                        g.unicode_any();
                    }
                }
                g.s("\r\n");
            }
            1 => {
                // A wide character at the last column: it needs a wrap.
                g.cup(r, cols);
                g.unicode_wide();
            }
            2 => {
                // A wide character in the last two cells: pending wrap.
                g.cup(r, cols.saturating_sub(1).max(1));
                g.cluster_from(WIDE_ONE);
            }
            3 => {
                // Over one half of a wide character.
                g.cup(r, c);
                g.cluster_from(WIDE_ONE);
                let half = if g.chance(1, 2) { c + 1 } else { c };
                g.cup(r, half);
                g.text("x");
            }
            4 => {
                for _ in 0..8 {
                    g.cluster_from(WIDE_ONE);
                }
                g.cup(r, c);
                let n = g.rng.range(1, 4);
                match g.rng.below(5) {
                    0 => g.csi(n, '@'),
                    1 => g.csi(n, 'P'),
                    2 => g.csi(n, 'X'),
                    3 => g.csi(1, 'K'),
                    _ => g.csi(1, 'J'),
                }
            }
            5 => {
                g.unicode_wide();
                g.s("\x08");
                g.text("y");
            }
            6 => {
                // A line of mixed width that wraps.
                let mut cells = 0;
                while cells < cols + 5 {
                    if g.chance(1, 3) {
                        g.cluster_from(WIDE_ONE);
                        cells += 2;
                    } else {
                        let w = g.word();
                        g.text(w);
                        cells += w.len() as u32;
                    }
                }
            }
            7 => {
                g.cup(r, cols);
                let kind = g.rng.pick(&[4, 5]);
                g.unicode_item(kind);
            }
            8 => {
                g.csi(4, 'h');
                g.cup(r, c);
                g.text("ab");
                g.unicode_wide();
                g.csi(4, 'l');
            }
            9 => {
                g.unicode_wide();
                let n = g.rng.range(1, 6);
                g.csi(n, 'b');
            }
            _ => {
                g.text("e");
                for _ in 0..g.rng.range(2, 5) {
                    g.cluster_from(&["\u{300}", "\u{301}", "\u{302}", "\u{323}", "\u{20d7}"]);
                }
            }
        }
    }
}

/// Mode toggles, with some text in between.
fn modes(g: &mut Gen) {
    for _ in 0..g.rng.range(4, 12) {
        match g.rng.below(8) {
            0..=3 => {
                let m = g.rng.pick(TOGGLES);
                let on = g.chance(1, 2);
                g.mode(m, on);
            }
            4 => {
                let m = g.rng.pick(&[4, 20, 2, 12]);
                let fin = if g.chance(1, 2) { 'h' } else { 'l' };
                g.csi(m, fin);
            }
            5 => {
                let n = if g.chance(1, 12) {
                    g.rng.range(7, 12)
                } else {
                    g.rng.below(7)
                };
                g.csi(format!("{n} "), 'q');
            }
            6 => {
                let k = if g.chance(1, 2) { "\x1b=" } else { "\x1b>" };
                g.s(k);
            }
            _ => {
                let n = g.rng.range(1, 3);
                let t = g.words(n);
                g.text(&t);
            }
        }
    }
}

/// DEC special graphics and the other charsets, with every way to invoke them.
fn charset(g: &mut Gen) {
    const ACS: &str = "`afgjklmnopqrstuvwxyz{|}~_";
    for _ in 0..g.rng.range(2, 5) {
        match g.rng.below(8) {
            0 => {
                g.s("\x1b(0");
                g.text("lqqqqqk");
                g.newline();
                g.text("x     x");
                g.newline();
                g.text("mqqqqqj");
                g.s("\x1b(B");
            }
            1 => {
                g.s("\x1b)0");
                g.s("\x0e");
                g.text(ACS);
                g.s("\x0f");
                g.text(ACS);
            }
            2 => {
                g.s("\x1b*0");
                g.s("\x1bn");
                g.text(ACS);
                g.s("\x0f");
            }
            3 => {
                g.s("\x1b+0");
                g.s("\x1bo");
                g.text(ACS);
                g.s("\x0f");
            }
            4 => {
                // Designated, not invoked: the text stays ASCII.
                g.s("\x1b)0");
                g.text("lqk");
            }
            5 => {
                g.s("\x1b(A");
                g.text("#$£");
                g.s("\x1b(B");
            }
            6 => {
                // The charset is part of the saved cursor.
                g.s("\x1b(0");
                g.s("\x1b7");
                g.s("\x1b(B");
                g.text("abc");
                g.s("\x1b8");
                g.text("lqk");
            }
            _ => {
                g.s("\x1b(0");
                g.text(ACS);
            }
        }
    }
    if g.chance(3, 4) {
        for s in ["\x1b(B", "\x1b)B", "\x1b*B", "\x1b+B", "\x0f"] {
            g.s(s);
        }
    }
}

/// OSC titles and the title stack.
fn title(g: &mut Gen) {
    match g.rng.below(5) {
        0 => {
            for _ in 0..g.rng.range(1, 3) {
                let ps = g.rng.pick(&[0, 1, 2]);
                g.osc_title(ps);
            }
        }
        1 => {
            let k = g.rng.range(1, 5);
            for _ in 0..k {
                g.s("\x1b[22;0t");
                g.osc_title(2);
            }
            for _ in 0..k {
                g.s("\x1b[23;0t");
            }
        }
        2 => {
            // Many pushes.
            for _ in 0..g.rng.range(20, 60) {
                let which = g.rng.pick(&["22;0", "22;0", "22;1", "22;2"]);
                g.csi(which, 't');
                if g.chance(1, 4) {
                    g.osc_title(0);
                }
            }
            for _ in 0..g.rng.range(0, 30) {
                g.s("\x1b[23;0t");
            }
        }
        3 => {
            // More pops than pushes.
            for _ in 0..g.rng.range(1, 6) {
                let which = g.rng.pick(&["23;0", "23;0", "23;1", "23;2"]);
                g.csi(which, 't');
            }
        }
        _ => {
            g.s("\x1b[22;0t");
            g.osc_title(2);
            g.s("\x1b[22;0t");
            g.osc_title(0);
            for _ in 0..3 {
                g.s("\x1b[23;0t");
            }
        }
    }
}

/// OSC 4, 10, 11, 12 and their resets, and text in the colours.
fn palette(g: &mut Gen) {
    for _ in 0..g.rng.range(2, 6) {
        match g.rng.below(8) {
            0..=2 => g.osc4(0),
            3 => {
                let ps = g.rng.pick(&[10, 11, 12, 17, 19]);
                g.osc_color(ps);
            }
            4 => {
                let ps = g.rng.pick(&[104, 110, 111, 112, 117, 119]);
                g.osc_reset(ps);
            }
            5 => {
                let n = g.rng.below(256);
                g.osc(&format!("4;{n};?"));
            }
            _ => {
                let (n, m) = (g.byte(), g.byte());
                g.sgr(format!("38;5;{n};48;5;{m}"));
                let w = g.word();
                g.text(w);
                g.sgr(0);
            }
        }
    }
}

/// A stack that is pushed past its limit: the kitty keyboard stack and the title stack of
/// `alacritty_terminal` hold 4096 entries each, and a push on a full stack drops the oldest entry.
/// The two kinds come in turn. It is rare (about once for each 300 KB) and it is about 30 KB.
///
/// A push on a full keyboard stack also removes the oldest title from the title stack, and that
/// panics when the title stack is empty (a bug in `alacritty_terminal` 0.26.0). One in two of the
/// keyboard scenes starts with RIS, which empties both stacks, so that the engine meets the bug.
fn deep_stack(g: &mut Gen, count: u32) {
    if count.is_multiple_of(2) {
        if g.chance(1, 2) {
            g.s("\x1bc");
        }
        for _ in 0..g.rng.range(4097, 4140) {
            g.kitty_push(0);
        }
        if g.chance(1, 2) {
            for _ in 0..g.rng.range(1, 20) {
                g.s("\x1b[<u");
            }
        }
    } else {
        for i in 0..g.rng.range(4097, 4140) {
            g.s("\x1b[22;0t");
            if i % 61 == 0 {
                g.osc_title(2);
            }
        }
        if g.chance(1, 2) {
            for _ in 0..g.rng.range(1, 20) {
                g.s("\x1b[23;0t");
            }
        }
    }
}

/// A resize, with the state around it that a resize disturbs. It is the only source of
/// `Op::Resize`, once for each 50 to 100 KB.
fn resize_scene(g: &mut Gen) {
    // Half of the resizes happen on the alternate screen.
    match (g.alt, g.chance(1, 2)) {
        (false, true) => g.mode(1049, true),
        (true, true) => g.mode(1049, false),
        _ => {}
    }
    let (cols, rows) = (g.cols32(), g.rows32());
    match g.rng.below(6) {
        0 => {
            // Pending wrap.
            let r = g.row_in();
            g.cup(r, 1);
            let line = g.fill(cols as usize);
            g.text(&line);
        }
        1 => {
            g.decstbm(0);
            g.mode(6, true);
            g.csi("", 'H');
            let t = g.words(2);
            g.text(&t);
        }
        2 => {
            // A saved cursor far from the origin, and a cursor elsewhere.
            g.cup(rows, cols);
            g.s("\x1b7");
            g.cup(1, 1);
        }
        3 => {
            for _ in 0..g.rng.range(2, 6) {
                let n = g.rng.range(1, cols * 2).min(300) as usize;
                let line = g.fill(n);
                g.text(&line);
                g.newline();
            }
        }
        4 => {
            let r = g.row_in();
            g.cup(r, cols);
            g.unicode_wide();
        }
        _ => {
            g.csi(3, 'g');
            for c in [5, 20, 30] {
                g.csi(c, 'G');
                g.s("\x1bH");
            }
        }
    }
    let (new_cols, new_rows) = new_size(g);
    g.resize(new_cols, new_rows);
    // After the resize the corpus uses the new size.
    if g.chance(1, 2) {
        g.mode(6, false);
    }
    let (r, c) = (g.row_in(), g.col_in());
    g.cup(r, c);
    let t = g.words(2);
    g.text(&t);
    if g.chance(1, 3) {
        g.s("\x1b8");
        g.text("!");
    }
}

fn new_size(g: &mut Gen) -> (u16, u16) {
    fn dimension(rng: &mut Rng, cur: u16, min: u16, max: u16) -> u16 {
        let (cur, min, max) = (u32::from(cur), u32::from(min), u32::from(max));
        let shrink = |rng: &mut Rng| rng.range(min, cur.saturating_sub(1).max(min));
        let grow = |rng: &mut Rng| rng.range((cur + 1).min(max), max);
        let n = match rng.below(20) {
            0..=6 if cur > min => shrink(rng),
            0..=6 => grow(rng),
            7..=13 if cur < max => grow(rng),
            7..=13 => shrink(rng),
            14..=17 => rng.range(min, max),
            18 => min,
            _ => max,
        };
        // The terminal can start outside the range (see `corpus_from`); a resize never does.
        n.clamp(min, max) as u16
    }
    let (old_cols, old_rows) = (g.cols, g.rows);
    let mut cols = dimension(&mut g.rng, old_cols, MIN_COLS, MAX_COLS);
    let rows = dimension(&mut g.rng, old_rows, MIN_ROWS, MAX_ROWS);
    if (cols, rows) == (old_cols, old_rows) {
        cols = if old_cols < MAX_COLS {
            old_cols + 1
        } else {
            old_cols - 1
        };
    }
    (cols, rows)
}

/// Draws a few sequences from the deck. Every kind of sequence appears once for each pass.
fn salad(g: &mut Gen) {
    g.salads += 1;
    if g.salads.is_multiple_of(24) {
        let item = g.rare.draw(&mut g.rng);
        g.emit(item);
    }
    for _ in 0..g.rng.range(8, 28) {
        let item = g.draw();
        g.emit(item);
        if g.chance(1, 3) {
            let n = g.rng.range(1, 2);
            let t = g.words(n);
            g.text(&t);
        }
    }
}

/// Text that fills a row exactly, and what comes after it.
fn pending_wrap(g: &mut Gen) {
    let cols = g.cols32();
    for _ in 0..g.rng.range(1, 3) {
        let r = g.row_in();
        let autowrap_off = g.chance(1, 6);
        if autowrap_off {
            g.mode(7, false);
        }
        g.cup(r, 1);
        let line = if g.chance(1, 5) {
            // Wide characters: exactly `cols` cells.
            let mut cells = 0;
            let mut line = String::new();
            while cells < cols {
                if cells + 2 <= cols && g.chance(1, 3) {
                    line.push_str(g.rng.pick(WIDE_ONE));
                    cells += 2;
                } else {
                    line.push('a');
                    cells += 1;
                }
            }
            line
        } else {
            g.fill(cols as usize)
        };
        g.text(&line);
        match g.rng.below(14) {
            0 => g.text("X"),
            1 => g.s("\r"),
            2 => g.s("\n"),
            3 => g.csi(6, 'n'),
            4 => g.csi("", 'C'),
            5 => g.s("\x08"),
            6 => g.s("\t"),
            7 => {
                g.sgr("1;31");
                g.text("Y");
            }
            8 => g.csi("", 'K'),
            9 => g.csi(3, 'X'),
            10 => g.csi(2, '@'),
            11 => {
                g.s("\x1b7");
                let r2 = g.row_in();
                g.cup(r2, 1);
                g.text("w");
                g.s("\x1b8");
                g.text("Z");
            }
            12 => g.csi(3, 'b'),
            _ => {
                g.csi(4, 'h');
                g.text("I");
                g.csi(4, 'l');
            }
        }
        if autowrap_off {
            // The last cell is overwritten again and again.
            g.text("abc");
            g.mode(7, true);
        } else if g.chance(1, 3) {
            let t = g.fill(5);
            g.text(&t);
        }
    }
}

/// Scroll regions, origin mode, and every way to scroll.
fn scroll_region(g: &mut Gen) {
    let rows = g.rows32();
    let top = g.rng.range(1, rows.saturating_sub(1).max(1));
    let bottom = g.rng.range(top + 1, rows).max(top);
    g.csi(format!("{top};{bottom}"), 'r');
    let origin = g.chance(1, 2);
    if origin {
        g.mode(6, true);
    }
    let height = bottom - top + 1;
    for _ in 0..g.rng.range(3, 8) {
        let r = g.rng.range(1, height);
        let c = g.col_in();
        g.cup(r, c);
        let n = g.rng.range(1, 3);
        let t = g.words(n);
        g.text(&t);
    }
    for _ in 0..g.rng.range(2, 6) {
        match g.rng.below(8) {
            0 => {
                g.cup(height, 1);
                for _ in 0..g.rng.range(1, 4) {
                    g.newline();
                }
            }
            1 => {
                g.cup(1, 1);
                for _ in 0..g.rng.range(1, 4) {
                    g.s("\x1bM");
                }
            }
            2 => {
                let n = g.count(height);
                g.csi(n, 'S');
            }
            3 => {
                let n = g.count(height);
                g.csi(n, 'T');
            }
            4 => {
                let n = g.rng.range(1, 3);
                g.csi(n, 'L');
            }
            5 => {
                let n = g.rng.range(1, 3);
                g.csi(n, 'M');
            }
            6 => {
                let k = g.rng.pick(&["\x1bD", "\x1bE", "\x1bM"]);
                for _ in 0..g.rng.range(1, 5) {
                    g.s(k);
                }
            }
            _ => {
                let r = g.rng.range(1, height);
                g.cup(r, 1);
                let w = g.word();
                g.text(w);
            }
        }
    }
    if origin && g.chance(3, 4) {
        g.mode(6, false);
    }
    if g.chance(1, 2) {
        g.csi("", 'r');
    }
}

/// Lay down text, then insert, delete and erase in it.
fn editing(g: &mut Gen) {
    let cols = g.cols32();
    for _ in 0..g.rng.range(2, 5) {
        let r = g.row_in();
        g.cup(r, 1);
        let n = g.rng.range(cols / 2, cols).max(1) as usize;
        let line = g.fill(n);
        g.text(&line);
    }
    for _ in 0..g.rng.range(4, 10) {
        let (r, c) = (g.row(), g.col());
        g.cup(r, c);
        match g.rng.below(13) {
            0 => g.rel('@' as u16),
            1 => g.rel('P' as u16),
            2 => g.rel('X' as u16),
            3 => {
                let n = g.rng.pick(&[0, 0, 1, 2]);
                g.el(n);
            }
            4 => {
                let n = g.rng.pick(&[0, 0, 0, 1, 1, 1, 2, 3]);
                g.ed(n);
            }
            5 => g.rel('L' as u16),
            6 => g.rel('M' as u16),
            7 => {
                g.csi(4, 'h');
                let t = g.words(2);
                g.text(&t);
                g.csi(4, 'l');
            }
            8 => {
                g.csi(20, 'h');
                g.text("ab");
                g.s("\n");
                g.text("cd");
                g.csi(20, 'l');
            }
            9 => {
                let k = if g.chance(1, 2) { 'K' } else { 'J' };
                let n = g.rng.below(3);
                g.csi(format!("?{n}"), k);
            }
            10 => {
                let t = g.words(2);
                g.text(&t);
            }
            11 => {
                // Erase with a background colour.
                let n = g.rng.below(8);
                g.sgr(format!("4{n}"));
                g.csi("", 'K');
                g.sgr(0);
            }
            _ => {
                let n = g.rng.below(8);
                g.sgr(format!("4{n}"));
                g.rel('X' as u16);
                g.sgr(0);
            }
        }
    }
}

/// Tab stops: set, clear, jump forward and backward.
fn tabs(g: &mut Gen) {
    let cols = g.cols32();
    if g.chance(1, 2) {
        g.csi(3, 'g');
    }
    for _ in 0..g.rng.range(2, 6) {
        let c = g.col_in();
        g.csi(c, 'G');
        g.s("\x1bH");
    }
    let r = g.row_in();
    g.cup(r, 1);
    for _ in 0..g.rng.range(2, 7) {
        g.s("\t");
        g.text("a");
    }
    let n = g.rng.range(1, 4);
    g.csi(n, 'I');
    g.text("c");
    let n = g.rng.range(1, 4);
    g.csi(n, 'Z');
    g.text("d");
    if g.chance(1, 2) {
        g.csi(0, 'g');
        g.s("\t");
        g.text("e");
    }
    if g.chance(1, 3) {
        // Back to a stop at every eighth column.
        g.csi(3, 'g');
        for c in (9..=cols).step_by(8) {
            g.csi(c, 'G');
            g.s("\x1bH");
        }
    }
    if g.chance(1, 6) {
        let n = g.rng.pick(&[1, 2, 4, 5]);
        g.csi(n, 'g');
    }
}

/// Kitty keyboard stack, modifyOtherKeys, mouse modes and encodings.
fn keyboard_mouse(g: &mut Gen) {
    for _ in 0..g.rng.range(3, 8) {
        match g.rng.below(11) {
            0 => g.kitty_push(0),
            1 => {
                // More pushes than the engine keeps, and no pop.
                for _ in 0..g.rng.range(18, 40) {
                    g.kitty_push(0);
                }
            }
            2 => g.kitty_pop(0),
            3 => g.kitty_set(0),
            4 => g.modify_other_keys(0),
            5 => {
                let m = g.rng.pick(&[9, 1000, 1002, 1003]);
                let e = g.rng.pick(&[1005, 1006, 1015]);
                g.mode(m, true);
                g.mode(e, true);
            }
            6 => {
                let m = g.rng.pick(&[9, 1000, 1002, 1003]);
                let e = g.rng.pick(&[1005, 1006, 1015]);
                g.mode(m, false);
                if g.chance(1, 2) {
                    g.mode(e, false);
                }
            }
            7 => {
                let (on, keypad) = (g.chance(1, 2), g.chance(1, 2));
                g.mode(1, on);
                g.s(if keypad { "\x1b=" } else { "\x1b>" });
            }
            8 => {
                let m = g.rng.pick(&[1004, 1007, 2004]);
                let on = g.chance(1, 2);
                g.mode(m, on);
            }
            9 => g.s("\x1b[?u"),
            _ => {
                let t = g.words(1);
                g.text(&t);
            }
        }
    }
}

/// Hyperlinks, clipboard writes and notifications.
fn links(g: &mut Gen) {
    for _ in 0..g.rng.range(1, 4) {
        match g.rng.below(6) {
            0 | 1 => g.hyperlink(0),
            2 => {
                // A link over a line break, not closed.
                let url = g.url();
                g.osc(&format!("8;;{url}"));
                let t = g.words(2);
                g.text(&t);
                g.newline();
                let t = g.words(1);
                g.text(&t);
            }
            3 => {
                // A link that wraps, and one that replaces another.
                let cols = g.cols32();
                let url = g.url();
                g.osc(&format!("8;;{url}"));
                let t = g.fill((cols + 10) as usize);
                g.text(&t);
                let url = g.url();
                g.osc(&format!("8;id=n;{url}"));
                let t = g.words(1);
                g.text(&t);
                g.osc("8;;");
            }
            4 => g.clipboard(0),
            _ => {
                let k = g.rng.below(2) as u16;
                g.notify(k);
            }
        }
    }
}

/// Queries. The engine answers some of them; the answers do not change the corpus.
fn queries(g: &mut Gen) {
    for _ in 0..g.rng.range(3, 7) {
        let q = g.rng.pick(QUERIES);
        g.s(q);
    }
    if g.chance(1, 2) {
        let m = g.rng.pick(MODES);
        g.csi(format!("?{m}$"), 'p');
    }
}

/// A synchronized update, sometimes never closed before the next scene.
fn sync_update(g: &mut Gen) {
    g.csi("?2026", 'h');
    for _ in 0..g.rng.range(4, 12) {
        let (r, c) = (g.row_in(), g.col_in());
        g.cup(r, c);
        if g.chance(1, 2) {
            let s = g.sgr_item();
            g.sgr(s);
        }
        let n = g.rng.range(1, 3);
        let t = g.words(n);
        g.text(&t);
    }
    if !g.chance(1, 4) {
        g.csi("?2026", 'l');
    }
}

/// REP after text, after other sequences, after a wide character and at the margin.
fn repeat(g: &mut Gen) {
    for _ in 0..g.rng.range(2, 5) {
        let variant = g.rng.below(4) as u16;
        g.rep(variant);
        if g.chance(1, 3) {
            g.newline();
        }
    }
}

fn invalid_atom(g: &mut Gen) {
    match g.rng.below(9) {
        0 => {
            // A continuation byte on its own.
            let b = 0x80 + g.rng.below(64) as u8;
            g.atom(&[b]);
        }
        1 => {
            let b = g.rng.pick(&[0xC0, 0xC1, 0xF5, 0xF8, 0xFE, 0xFF]);
            g.atom(&[b]);
        }
        2 => {
            // A lead byte cut short, closed by an ASCII character in the same atom.
            let atom: &[u8] = match g.rng.below(3) {
                0 => &[0xC3, b'a'],
                1 => &[0xE3, 0x81, b'b'],
                _ => &[0xF0, 0x9F, 0x98, b'c'],
            };
            g.atom(atom);
        }
        3 => {
            let atom: &[u8] = match g.rng.below(3) {
                0 => &[0xC0, 0xAF],
                1 => &[0xE0, 0x80, 0xAF],
                _ => &[0xED, 0xA0, 0x80],
            };
            g.atom(atom);
        }
        4 => {
            let b = g.rng.pick(&[
                0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16,
                0x17, 0x19, 0x1c, 0x1d, 0x1e, 0x1f, 0x7f,
            ]);
            g.atom(&[b]);
        }
        5 => {
            // A sequence that CAN or SUB cancels.
            let s = g.rng.pick(&[
                "\x1b[1;2\x18",
                "\x1b[3\x1a",
                "\x1b]0;abc\x18",
                "\x1b[?25\x1a",
            ]);
            g.s(s);
        }
        6 => g.s("\u{fffd}"),
        7 => g.s("\u{e000}\u{10ffff}"),
        _ => {
            let n = g.rng.range(1, 6);
            let t = g.noise(n);
            g.text(&t);
        }
    }
}

/// A small share of bytes that are not valid output.
fn invalid(g: &mut Gen) {
    for _ in 0..g.rng.range(2, 6) {
        invalid_atom(g);
        if g.chance(1, 2) {
            let t = g.words(1);
            g.text(&t);
        }
    }
}

/// Lines in many styles.
fn sgr_soup(g: &mut Gen) {
    for _ in 0..g.rng.range(6, 16) {
        let k = g.rng.range(1, 4);
        let params: Vec<String> = (0..k).map(|_| g.sgr_item()).collect();
        g.sgr(params.join(";"));
        let w = g.word();
        g.text(w);
        g.text(" ");
    }
    if g.chance(1, 3) {
        g.sgr(0);
    }
}

/// Cursor movements of every kind, each followed by a mark.
fn cursor_dance(g: &mut Gen) {
    for _ in 0..g.rng.range(6, 16) {
        match g.rng.below(13) {
            0 => g.emit_cup(0),
            1 => g.emit_hvp(0),
            2..=9 => {
                let fin = g
                    .rng
                    .pick(&['A', 'B', 'C', 'D', 'E', 'F', 'G', 'd', '`', 'a', 'e']);
                g.rel(fin as u16);
            }
            10 => g.s("\x1b7"),
            11 => g.s("\x1b8"),
            _ => {
                let save = g.chance(1, 2);
                g.s(if save { "\x1b[s" } else { "\x1b[u" });
            }
        }
        let n = g.rng.range(1, 3);
        let mark = g.noise(n);
        g.text(&mark);
    }
}
