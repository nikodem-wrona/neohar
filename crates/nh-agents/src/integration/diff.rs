//! A unified diff of two texts, for `--dry-run`.

/// How many lines of context stand around a change.
const CONTEXT: usize = 3;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Op<'a> {
    Same(&'a str),
    Del(&'a str),
    Add(&'a str),
}

/// The operations that turn `old` into `new`, from the longest common subsequence of the lines.
fn operations<'a>(old: &[&'a str], new: &[&'a str]) -> Vec<Op<'a>> {
    let (n, m) = (old.len(), new.len());
    // `table[i][j]` is the length of the common part of `old[i..]` and `new[j..]`.
    let mut table = vec![vec![0_u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            table[i][j] = if old[i] == new[j] {
                table[i + 1][j + 1] + 1
            } else {
                table[i + 1][j].max(table[i][j + 1])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    let mut out = Vec::with_capacity(n.max(m));
    while i < n && j < m {
        if old[i] == new[j] {
            out.push(Op::Same(old[i]));
            i += 1;
            j += 1;
        } else if table[i + 1][j] >= table[i][j + 1] {
            out.push(Op::Del(old[i]));
            i += 1;
        } else {
            out.push(Op::Add(new[j]));
            j += 1;
        }
    }
    out.extend(old[i..].iter().map(|line| Op::Del(line)));
    out.extend(new[j..].iter().map(|line| Op::Add(line)));
    out
}

/// The lines of a text, each without its line end.
fn lines(text: &str) -> Vec<&str> {
    text.lines().collect()
}

/// The diff in the unified format with the names of the two files. An empty text means no change.
pub fn unified(old_name: &str, new_name: &str, old: &str, new: &str) -> String {
    if old == new {
        return String::new();
    }
    let ops = operations(&lines(old), &lines(new));
    let mut out = format!("--- {old_name}\n+++ {new_name}\n");
    let changed: Vec<usize> = ops
        .iter()
        .enumerate()
        .filter(|(_, op)| !matches!(op, Op::Same(_)))
        .map(|(i, _)| i)
        .collect();
    let mut at = 0;
    while at < changed.len() {
        // A hunk runs from the context before the first change to the context after the last one
        // that is near enough to join.
        let mut last = at;
        while last + 1 < changed.len() && changed[last + 1] - changed[last] <= 2 * CONTEXT {
            last += 1;
        }
        let from = changed[at].saturating_sub(CONTEXT);
        let to = (changed[last] + CONTEXT + 1).min(ops.len());
        let before = ops[..from]
            .iter()
            .filter(|op| !matches!(op, Op::Add(_)))
            .count();
        let after = ops[..from]
            .iter()
            .filter(|op| !matches!(op, Op::Del(_)))
            .count();
        let old_len = ops[from..to]
            .iter()
            .filter(|op| !matches!(op, Op::Add(_)))
            .count();
        let new_len = ops[from..to]
            .iter()
            .filter(|op| !matches!(op, Op::Del(_)))
            .count();
        let start = |first: usize, len: usize| if len == 0 { first } else { first + 1 };
        out.push_str(&format!(
            "@@ -{},{old_len} +{},{new_len} @@\n",
            start(before, old_len),
            start(after, new_len)
        ));
        for op in &ops[from..to] {
            match op {
                Op::Same(line) => out.push_str(&format!(" {line}\n")),
                Op::Del(line) => out.push_str(&format!("-{line}\n")),
                Op::Add(line) => out.push_str(&format!("+{line}\n")),
            }
        }
        at = last + 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_texts_have_no_diff() {
        assert_eq!(unified("a", "b", "x\ny\n", "x\ny\n"), "");
    }

    #[test]
    fn an_added_block_shows_with_context() {
        let old = "1\n2\n3\n4\n5\n6\n7\n8\n";
        let new = "1\n2\n3\n4\nnew\n5\n6\n7\n8\n";
        assert_eq!(
            unified("f", "f", old, new),
            "--- f\n+++ f\n@@ -2,6 +2,7 @@\n 2\n 3\n 4\n+new\n 5\n 6\n 7\n"
        );
    }

    #[test]
    fn a_change_and_a_removal_are_marked() {
        let diff = unified("f", "f", "a\nb\nc\n", "a\nB\n");
        assert_eq!(diff, "--- f\n+++ f\n@@ -1,3 +1,2 @@\n a\n-b\n-c\n+B\n");
    }

    #[test]
    fn far_changes_make_two_hunks() {
        let old: String = (1..=30).map(|n| format!("{n}\n")).collect();
        let new = old
            .replacen("2\n", "two\n", 1)
            .replacen("29\n", "twenty-nine\n", 1);
        let diff = unified("f", "f", &old, &new);
        assert_eq!(diff.matches("@@ -").count(), 2, "{diff}");
        assert!(diff.contains("-2\n+two\n") && diff.contains("-29\n+twenty-nine\n"));
    }

    #[test]
    fn a_new_file_is_all_additions() {
        let diff = unified("/dev/null", "f", "", "{\n}\n");
        assert_eq!(diff, "--- /dev/null\n+++ f\n@@ -0,0 +1,2 @@\n+{\n+}\n");
    }
}
