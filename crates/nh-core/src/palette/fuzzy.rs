//! Matches a query against the title of a command.
//!
//! The query matches when its characters are in the title in the same order, without regard to
//! case. Spaces in the query are ignored. The score prefers runs of characters, and characters at
//! the start of a word, and it takes a little for each character that the match skips. The match
//! with the best score wins, and it gives the ranges that the page highlights, in UTF-16 code
//! units because the page slices strings that way.

/// What a title scores for a query.
#[derive(Clone, Debug, PartialEq)]
pub struct Match {
    pub score: f32,
    /// The matched ranges of the title, as `[start, end)` in UTF-16 code units.
    pub ranges: Vec<(u32, u32)>,
}

const MATCH: i32 = 10;
const RUN: i32 = 15;
const WORD_START: i32 = 12;
const SAME_CASE: i32 = 1;
const GAP: i32 = 1;
/// A match that starts late loses at most this much.
const LEADING_GAP_MAX: i32 = 8;

fn starts_word(title: &[char], at: usize) -> bool {
    at == 0 || matches!(title[at - 1], ' ' | '/' | '.' | '-' | '_' | ':')
}

fn same(a: char, b: char) -> bool {
    a == b || a.to_lowercase().eq(b.to_lowercase())
}

/// The best match of `query` in `title`, or `None` when the query is not in the title.
pub fn match_title(query: &str, title: &str) -> Option<Match> {
    let wanted: Vec<char> = query.chars().filter(|c| !c.is_whitespace()).collect();
    if wanted.is_empty() {
        return Some(Match {
            score: 0.0,
            ranges: Vec::new(),
        });
    }
    let chars: Vec<char> = title.chars().collect();
    let (n, m) = (chars.len(), wanted.len());
    if m > n {
        return None;
    }
    // `best[j][i]`: the best score with `wanted[j]` matched at `chars[i]`, and where `wanted[j-1]`
    // was matched. `None` when `wanted[j]` cannot be there.
    let mut best: Vec<Vec<Option<(i32, usize)>>> = vec![vec![None; n]; m];
    for j in 0..m {
        for i in 0..n {
            if !same(wanted[j], chars[i]) {
                continue;
            }
            let mut base = MATCH;
            if wanted[j] == chars[i] {
                base += SAME_CASE;
            }
            if starts_word(&chars, i) {
                base += WORD_START;
            }
            if j == 0 {
                let lead = i32::try_from(i).unwrap_or(i32::MAX).min(LEADING_GAP_MAX) * GAP;
                best[0][i] = Some((base - lead, usize::MAX));
                continue;
            }
            // The best earlier match of the previous character. A run of two characters scores
            // more, and each skipped character costs a little.
            let choice = best[j - 1]
                .iter()
                .take(i)
                .enumerate()
                .filter_map(|(k, entry)| {
                    let (score, _) = (*entry)?;
                    let run = if k + 1 == i { RUN } else { 0 };
                    let gap = i32::try_from(i - k - 1).unwrap_or(i32::MAX) * GAP;
                    Some((score + base + run - gap, k))
                })
                .fold(None, |top: Option<(i32, usize)>, candidate| match top {
                    Some(current) if current.0 >= candidate.0 => top,
                    _ => Some(candidate),
                });
            best[j][i] = choice;
        }
    }
    // The best end of the match, and the way back to its start.
    let (mut at, (score, _)) = (0..n)
        .filter_map(|i| best[m - 1][i].map(|entry| (i, entry)))
        .fold(
            None,
            |top: Option<(usize, (i32, usize))>, candidate| match top {
                Some((_, (current, _))) if current >= candidate.1.0 => top,
                _ => Some(candidate),
            },
        )?;
    let mut matched = vec![at];
    for j in (1..m).rev() {
        at = best[j][at]?.1;
        matched.push(at);
    }
    matched.reverse();
    // The offsets in UTF-16 code units.
    let mut offsets = Vec::with_capacity(n + 1);
    let mut units = 0u32;
    for c in &chars {
        offsets.push(units);
        units += u32::try_from(c.len_utf16()).unwrap_or(1);
    }
    offsets.push(units);
    let mut ranges: Vec<(u32, u32)> = Vec::new();
    for &index in &matched {
        let (start, end) = (offsets[index], offsets[index + 1]);
        match ranges.last_mut() {
            Some(last) if last.1 == start => last.1 = end,
            _ => ranges.push((start, end)),
        }
    }
    Some(Match {
        score: score as f32,
        ranges,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ranges(query: &str, title: &str) -> Option<Vec<(u32, u32)>> {
        match_title(query, title).map(|m| m.ranges)
    }

    #[test]
    fn an_empty_query_matches_everything_without_ranges() {
        let m = match_title("", "New tab").expect("a match");
        assert_eq!(m.score, 0.0);
        assert!(m.ranges.is_empty());
        assert!(match_title("  ", "New tab").is_some());
    }

    #[test]
    fn the_characters_must_be_in_order() {
        assert!(match_title("nta", "New tab").is_some());
        assert!(match_title("tn", "New tab").is_none());
        assert!(match_title("bat", "New tab").is_none());
        assert!(match_title("x", "New tab").is_none());
        assert!(match_title("new tabs", "New tab").is_none());
    }

    #[test]
    fn the_case_does_not_matter_and_a_space_in_the_query_is_ignored() {
        assert_eq!(ranges("NEW TAB", "New tab"), Some(vec![(0, 3), (4, 7)]));
        assert_eq!(ranges("newtab", "New tab"), Some(vec![(0, 3), (4, 7)]));
    }

    #[test]
    fn a_run_and_the_start_of_a_word_win() {
        // `nt` matches `N` and `t` of `New tab`, both at the start of a word.
        assert_eq!(ranges("nt", "New tab"), Some(vec![(0, 1), (4, 5)]));
        // `tab` is one run.
        assert_eq!(ranges("tab", "Close tab"), Some(vec![(6, 9)]));
        // A word start beats a character in the middle of a word.
        let split = match_title("sp", "Split pane").expect("a match");
        assert_eq!(split.ranges, vec![(0, 2)]);
    }

    #[test]
    fn a_better_match_has_a_higher_score() {
        let exact = match_title("close tab", "Close tab")
            .expect("a match")
            .score;
        let scattered = match_title("close tab", "Close the other tab")
            .expect("a match")
            .score;
        assert!(exact > scattered, "{exact} {scattered}");
        let start = match_title("n", "New tab").expect("a match").score;
        let middle = match_title("n", "Open new tab").expect("a match").score;
        assert!(start > middle);
    }

    #[test]
    fn the_ranges_count_utf16_code_units() {
        // The emoji takes two units, and the space one.
        assert_eq!(ranges("tab", "🙂 tab"), Some(vec![(3, 6)]));
        assert_eq!(ranges("é", "Café"), Some(vec![(3, 4)]));
    }
}
