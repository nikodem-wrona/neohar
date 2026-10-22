//! The terminal sequences that the holder acts on: the queries it answers or queues
//! while no core is attached, and the modes it tracks to answer them.

use nh_proto::answers::QueryKind;

/// A sequence that the holder cares about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Sequence {
    /// `CSI ? Pn h` (`set`) or `CSI ? Pn l` for one private mode.
    Mode { number: u16, set: bool },
    /// `CSI > flags u`: push kitty keyboard flags.
    KittyPush(u8),
    /// `CSI < n u`: pop `n` entries.
    KittyPop(u8),
    /// `CSI = flags ; mode u`: change the flags on top of the stack.
    KittySet { flags: u8, mode: u8 },
    /// A query, with the bytes that the program sent.
    Query { kind: QueryKind, raw: Vec<u8> },
}

/// Parses `;`-separated decimal numbers. An empty field is `None`. Any other byte is an error.
fn numbers(params: &[u8]) -> Option<Vec<Option<u32>>> {
    params
        .split(|&b| b == b';')
        .map(|field| match field {
            [] => Some(None),
            digits if digits.iter().all(u8::is_ascii_digit) => {
                std::str::from_utf8(digits).ok()?.parse().ok().map(Some)
            }
            _ => None,
        })
        .collect()
}

/// Parses a CSI sequence. `body` is the bytes between `ESC [` and the final byte `fin`.
pub fn parse_csi(body: &[u8], fin: u8) -> Vec<Sequence> {
    let (marker, rest) = match body.first() {
        Some(&m) if (0x3C..=0x3F).contains(&m) => (Some(m), &body[1..]),
        _ => (None, body),
    };
    let split = rest
        .iter()
        .position(|b| (0x20..=0x2F).contains(b))
        .unwrap_or(rest.len());
    let (params, intermediate) = rest.split_at(split);
    let Some(nums) = numbers(params) else {
        return Vec::new();
    };
    let first = nums.first().copied().flatten();
    let one = nums.len() <= 1;
    let zero_or_none = one && first.is_none_or(|n| n == 0);
    let query = |kind| {
        vec![Sequence::Query {
            kind,
            raw: [b"\x1b[", body, &[fin]].concat(),
        }]
    };
    let small = |n: Option<u32>, default: u8| n.map_or(default, |n| u8::try_from(n).unwrap_or(0));
    match (marker, intermediate, fin) {
        (Some(b'?'), [], b'h' | b'l') => nums
            .iter()
            .flatten()
            .filter_map(|&n| u16::try_from(n).ok())
            .map(|number| Sequence::Mode {
                number,
                set: fin == b'h',
            })
            .collect(),
        (None, [], b'c') if zero_or_none => query(QueryKind::Da1),
        (Some(b'>'), [], b'c') if zero_or_none => query(QueryKind::Da2),
        (Some(b'='), [], b'c') if zero_or_none => query(QueryKind::Da3),
        (Some(b'>'), [], b'q') if zero_or_none => query(QueryKind::XtVersion),
        (m @ (None | Some(b'?')), [b'$'], b'p') if one => match first.map(u16::try_from) {
            Some(Ok(mode)) => query(QueryKind::Decrqm {
                private: m.is_some(),
                mode,
            }),
            _ => Vec::new(),
        },
        (None | Some(b'?'), [], b'n') if one && first == Some(6) => {
            query(QueryKind::CursorPosition)
        }
        (None, [], b't') if matches!(first, Some(14 | 16 | 18)) => query(QueryKind::WindowSize {
            op: first.map_or(0, |n| n as u16),
        }),
        (Some(b'>'), [], b'u') if one => vec![Sequence::KittyPush(small(first, 0))],
        (Some(b'<'), [], b'u') if one => vec![Sequence::KittyPop(small(first, 1))],
        (Some(b'='), [], b'u') if nums.len() <= 2 => vec![Sequence::KittySet {
            flags: small(first, 0),
            mode: small(nums.get(1).copied().flatten(), 1),
        }],
        _ => Vec::new(),
    }
}

/// Parses an OSC string. `body` is the text between `ESC ]` and the terminator, and `bell` tells
/// how the program ended it. Only the colour queries of OSC 10, 11 and 12 matter.
pub fn parse_osc(body: &[u8], bell: bool) -> Option<Sequence> {
    let code = match body {
        b"10;?" => 10,
        b"11;?" => 11,
        b"12;?" => 12,
        _ => return None,
    };
    let end: &[u8] = if bell { b"\x07" } else { b"\x1b\\" };
    Some(Sequence::Query {
        kind: QueryKind::OscColor { code },
        raw: [b"\x1b]", body, end].concat(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn csi(text: &str) -> Vec<Sequence> {
        let bytes = text.as_bytes();
        let (fin, body) = bytes.split_last().expect("a final byte");
        parse_csi(body, *fin)
    }

    fn kind(text: &str) -> Option<QueryKind> {
        match csi(text).as_slice() {
            [Sequence::Query { kind, .. }] => Some(*kind),
            _ => None,
        }
    }

    #[test]
    fn the_device_attribute_queries() {
        assert_eq!(kind("c"), Some(QueryKind::Da1));
        assert_eq!(kind("0c"), Some(QueryKind::Da1));
        assert_eq!(kind(">c"), Some(QueryKind::Da2));
        assert_eq!(kind(">0c"), Some(QueryKind::Da2));
        assert_eq!(kind("=c"), Some(QueryKind::Da3));
        assert_eq!(kind(">q"), Some(QueryKind::XtVersion));
        assert_eq!(kind("1c"), None, "a DA1 with a parameter is not a query");
    }

    #[test]
    fn a_query_keeps_the_bytes_that_the_program_sent() {
        let found = csi("?2004$p");
        assert_eq!(
            found,
            vec![Sequence::Query {
                kind: QueryKind::Decrqm {
                    private: true,
                    mode: 2004
                },
                raw: b"\x1b[?2004$p".to_vec(),
            }]
        );
        assert_eq!(
            kind("4$p"),
            Some(QueryKind::Decrqm {
                private: false,
                mode: 4
            })
        );
    }

    #[test]
    fn the_queries_that_need_the_screen() {
        assert_eq!(kind("6n"), Some(QueryKind::CursorPosition));
        assert_eq!(kind("?6n"), Some(QueryKind::CursorPosition));
        assert_eq!(kind("14t"), Some(QueryKind::WindowSize { op: 14 }));
        assert_eq!(kind("18t"), Some(QueryKind::WindowSize { op: 18 }));
        assert_eq!(kind("5n"), None);
        assert_eq!(kind("22t"), None);
    }

    #[test]
    fn private_modes_come_one_by_one() {
        assert_eq!(
            csi("?1004;2004h"),
            vec![
                Sequence::Mode {
                    number: 1004,
                    set: true
                },
                Sequence::Mode {
                    number: 2004,
                    set: true
                }
            ]
        );
        assert_eq!(
            csi("?2004l"),
            vec![Sequence::Mode {
                number: 2004,
                set: false
            }]
        );
        assert!(
            csi("2004h").is_empty(),
            "a mode without the `?` is not a private mode"
        );
    }

    #[test]
    fn the_kitty_keyboard_sequences() {
        assert_eq!(csi(">5u"), vec![Sequence::KittyPush(5)]);
        assert_eq!(csi(">u"), vec![Sequence::KittyPush(0)]);
        assert_eq!(csi("<u"), vec![Sequence::KittyPop(1)]);
        assert_eq!(csi("<3u"), vec![Sequence::KittyPop(3)]);
        assert_eq!(csi("=3;2u"), vec![Sequence::KittySet { flags: 3, mode: 2 }]);
        assert_eq!(csi("=3u"), vec![Sequence::KittySet { flags: 3, mode: 1 }]);
        assert!(csi("?u").is_empty());
    }

    #[test]
    fn malformed_parameters_are_ignored() {
        assert!(csi("6:1n").is_empty());
        assert!(csi("?99999999999h").is_empty());
        assert!(csi("x").is_empty());
    }

    #[test]
    fn the_osc_colour_queries() {
        let found = parse_osc(b"11;?", true);
        assert_eq!(
            found,
            Some(Sequence::Query {
                kind: QueryKind::OscColor { code: 11 },
                raw: b"\x1b]11;?\x07".to_vec()
            })
        );
        let found = parse_osc(b"10;?", false);
        assert!(matches!(found, Some(Sequence::Query { raw, .. }) if raw == b"\x1b]10;?\x1b\\"));
        assert_eq!(parse_osc(b"11;rgb:00/00/00", true), None);
        assert_eq!(parse_osc(b"0;title", true), None);
    }
}
