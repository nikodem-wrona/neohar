//! The 20 panes of the chaos run and the input that each kind of pane gets.
//!
//! The run starts `vim` ×3, `less` ×3, `htop` ×3, `nh-fixture tui` ×9 and `nh-fixture echo-seq`
//! ×2: the 20 panes of the gate. A 21st pane runs a shell loop that scrolls plain text. It does not
//! redraw, so the restart cannot make it exact, and the run reports its differences apart. The
//! random input is chosen so that it cannot harm anything: `htop` can send signals to
//! processes, so it gets keys that only change its view, and `less` and `vim` get no key that
//! leaves the program or starts another.

use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use serde_json::{Value, json};

use crate::rng::Rng;

/// What runs in a pane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Vim,
    Less,
    Htop,
    Tui,
    EchoSeq,
    /// A shell loop that prints lines for ever. It does not redraw, so a restart cannot bring it
    /// back exactly. The run counts its differences apart from the gate.
    Scroller,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Vim => "vim",
            Self::Less => "less",
            Self::Htop => "htop",
            Self::Tui => "tui",
            Self::EchoSeq => "echo-seq",
            Self::Scroller => "scroller",
        }
    }

    /// Whether the differences of this pane stay out of the verdict of the gate.
    pub fn informational(self) -> bool {
        self == Self::Scroller
    }
}

/// A pane of the run.
#[derive(Clone, Debug)]
pub struct PaneDef {
    pub kind: Kind,
    pub argv: Vec<String>,
    pub cols: u16,
    pub rows: u16,
}

/// The programs that the run needs.
pub struct Tools {
    pub vim: PathBuf,
    pub less: PathBuf,
    pub htop: PathBuf,
    pub fixture: PathBuf,
}

/// The first file called `name` in the directories of `PATH`.
fn find_in_path(name: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|dir| dir.join(name))
        .find(|path| path.is_file())
}

impl Tools {
    /// Finds `vim`, `less` and `htop` in `PATH`, and `nh-fixture` next to this program or at the
    /// path given.
    pub fn find(fixture: Option<PathBuf>) -> Result<Self> {
        let fixture = match fixture {
            Some(path) => path,
            None => std::env::current_exe()?
                .parent()
                .map(|dir| dir.join("nh-fixture"))
                .unwrap_or_default(),
        };
        if !fixture.is_file() {
            bail!(
                "{} does not exist; build it with `cargo build -p nh-fixture` or give --fixture",
                fixture.display()
            );
        }
        let need = |name: &str| {
            find_in_path(name)
                .ok_or_else(|| anyhow::anyhow!("{name} is not in PATH; the run needs it"))
        };
        Ok(Self {
            vim: need("vim")?,
            less: need("less")?,
            htop: need("htop")?,
            fixture,
        })
    }
}

/// The text files that `vim` and `less` open. The run makes them in `files`.
pub fn write_files(files: &Path) -> Result<()> {
    std::fs::create_dir_all(files)?;
    for n in 0..3 {
        let text: String = (0..300)
            .map(|i| {
                format!("vim file {n} line {i:04}: the quick brown fox jumps over the lazy dog\n")
            })
            .collect();
        std::fs::write(files.join(format!("doc-{n}.txt")), text)?;
    }
    let text: String = (0..4000)
        .map(|i| format!("less line {i:05} \u{2502} \u{1f980} wide and \u{e9}\u{301} combined text for the pager\n"))
        .collect();
    std::fs::write(files.join("big.txt"), text)?;
    Ok(())
}

/// The panes of the run, in the order in which they start. The sizes come from `rng`.
pub fn plan(tools: &Tools, files: &Path, rng: &mut Rng) -> Vec<PaneDef> {
    let path = |p: &Path| p.display().to_string();
    let fixture = path(&tools.fixture);
    let mut defs = Vec::new();
    let mut add = |kind, argv: Vec<String>, rng: &mut Rng| {
        defs.push(PaneDef {
            kind,
            argv,
            cols: u16::try_from(rng.range(70, 140)).unwrap_or(80),
            rows: u16::try_from(rng.range(20, 45)).unwrap_or(24),
        });
    };
    for n in 0..3 {
        add(
            Kind::Vim,
            vec![
                path(&tools.vim),
                "-u".into(),
                "NONE".into(),
                "-N".into(),
                "-n".into(),
                path(&files.join(format!("doc-{n}.txt"))),
            ],
            rng,
        );
    }
    for _ in 0..3 {
        add(
            Kind::Less,
            vec![path(&tools.less), path(&files.join("big.txt"))],
            rng,
        );
    }
    for _ in 0..3 {
        add(Kind::Htop, vec![path(&tools.htop)], rng);
    }
    for interval in [50, 80, 100, 150, 200, 100, 120, 60, 90] {
        add(
            Kind::Tui,
            vec![
                fixture.clone(),
                "tui".into(),
                "--interval-ms".into(),
                interval.to_string(),
            ],
            rng,
        );
    }
    for n in 0..2 {
        add(
            Kind::EchoSeq,
            vec![
                fixture.clone(),
                "echo-seq".into(),
                "--log".into(),
                path(&files.join(format!("echo-{n}.log"))),
            ],
            rng,
        );
    }
    add(
        Kind::Scroller,
        vec![
            "/bin/sh".into(),
            "-c".into(),
            "i=0; while :; do printf 'scroller %06d: some text for the pane to scroll and wrap, padded to a long line so that it wraps\\n' $i; i=$((i+1)); sleep 0.05; done".into(),
        ],
        rng,
    );
    defs
}

/// The payload of an input to a pane of `echo-seq`: ten ASCII bytes that nothing else produces.
pub fn payload(input_id: u64) -> String {
    format!("<{input_id:08}>")
}

pub fn text(text: &str) -> Value {
    json!({ "type": "text", "text": text })
}

pub fn key(name: &str) -> Value {
    json!({ "type": "key", "code": name, "key": name })
}

/// A random input for a pane that is not an `echo-seq` pane.
pub fn random_input(kind: Kind, rng: &mut Rng) -> Value {
    const ARROWS: [&str; 4] = ["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"];
    match kind {
        // Normal-mode motions and edits. No `:`, `Z` or `q`, which leave vim or record macros.
        Kind::Vim => match rng.below(10) {
            0 => key("Escape"),
            1 => key(rng.pick(&ARROWS)),
            2 => key(rng.pick(&["PageDown", "PageUp"])),
            3 => text(rng.pick(&["i", "a", "o"])),
            4 => text(rng.pick(&["hello", "x y z", "fn main() {}", "\u{e9}\u{4e16}"])),
            _ => text(rng.pick(&[
                "j", "k", "h", "l", "w", "b", "0", "$", "x", "u", "dd", "yy", "p", "gg", "G",
            ])),
        },
        // Pager keys. No `q`, and none of `v`, `!` and `:`, which start other programs.
        Kind::Less => match rng.below(6) {
            0 => key(rng.pick(&["PageDown", "PageUp"])),
            1 => key(rng.pick(&ARROWS)),
            _ => text(rng.pick(&["j", "k", " ", "b", "f", "g", "G", "d", "u"])),
        },
        // Keys that change the view. Not `k`, `F9` or Enter: they send signals to processes.
        Kind::Htop => match rng.below(4) {
            0 => key(rng.pick(&["ArrowUp", "ArrowDown"])),
            _ => text(rng.pick(&["P", "M", "T", "t", "H", "K", "I"])),
        },
        // Any key except `q`, which ends the program.
        Kind::Tui | Kind::EchoSeq | Kind::Scroller => match rng.below(4) {
            0 => key(rng.pick(&ARROWS)),
            1 => key("Escape"),
            _ => text(rng.pick(&["a", "b", "xyz", "0123", "\u{e9}", "hello world"])),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tools() -> Tools {
        Tools {
            vim: "/usr/bin/vim".into(),
            less: "/usr/bin/less".into(),
            htop: "/opt/homebrew/bin/htop".into(),
            fixture: "/x/nh-fixture".into(),
        }
    }

    #[test]
    fn the_plan_has_the_twenty_panes_of_the_gate() {
        let defs = plan(&tools(), Path::new("/f"), &mut Rng::new(1));
        let count = |kind| defs.iter().filter(|d| d.kind == kind).count();
        assert_eq!(defs.iter().filter(|d| !d.kind.informational()).count(), 20);
        assert_eq!(count(Kind::Scroller), 1, "one more pane, outside the gate");
        assert_eq!(
            (count(Kind::Vim), count(Kind::Less), count(Kind::Htop)),
            (3, 3, 3)
        );
        assert_eq!((count(Kind::Tui), count(Kind::EchoSeq)), (9, 2));
        assert!(
            defs.iter()
                .all(|d| (70..=140).contains(&d.cols) && (20..=45).contains(&d.rows))
        );
    }

    #[test]
    fn the_same_seed_gives_the_same_sizes() {
        let sizes = |seed| {
            plan(&tools(), Path::new("/f"), &mut Rng::new(seed))
                .iter()
                .map(|d| (d.cols, d.rows))
                .collect::<Vec<_>>()
        };
        assert_eq!(sizes(5), sizes(5));
        assert_ne!(sizes(5), sizes(6));
    }

    #[test]
    fn the_payloads_are_unique_and_ten_bytes_long() {
        assert_eq!(payload(7), "<00000007>");
        assert_eq!(payload(7).len(), 10);
        assert_ne!(payload(7), payload(70));
    }

    #[test]
    fn no_random_input_can_end_a_program_or_send_a_signal() {
        let mut rng = Rng::new(3);
        for kind in [
            Kind::Vim,
            Kind::Less,
            Kind::Htop,
            Kind::Tui,
            Kind::EchoSeq,
            Kind::Scroller,
        ] {
            for _ in 0..2000 {
                let input = random_input(kind, &mut rng);
                let text = input["text"].as_str().unwrap_or("");
                assert!(
                    !text.contains('q') && !text.contains('Q'),
                    "{kind:?}: {text}"
                );
                assert!(
                    !text.contains(':') && !text.contains('Z'),
                    "{kind:?}: {text}"
                );
                if kind == Kind::Htop {
                    assert!(!text.contains('k') && !text.contains('9'), "{text}");
                    assert_ne!(input["key"], "Enter");
                }
                if kind == Kind::Less {
                    assert!(!text.contains('v') && !text.contains('!'), "{text}");
                }
            }
        }
    }

    #[test]
    fn the_inputs_have_the_shape_of_the_api() {
        assert_eq!(text("a"), json!({ "type": "text", "text": "a" }));
        assert_eq!(
            key("Escape"),
            json!({ "type": "key", "code": "Escape", "key": "Escape" })
        );
        let parsed: nh_proto::input::LogicalInput =
            serde_json::from_value(key("ArrowUp")).expect("a key parses");
        assert!(matches!(parsed, nh_proto::input::LogicalInput::Key(_)));
        serde_json::from_value::<nh_proto::input::LogicalInput>(text("x")).expect("a text parses");
    }
}
