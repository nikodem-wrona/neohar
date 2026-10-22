//! The name of a task on disk and in git.

/// The longest slug, suffix included.
pub const MAX_LEN: usize = 40;

/// A title as a slug: lowercase letters and digits, joined by single hyphens. A title with no such
/// character gives `task`.
pub fn slugify(title: &str) -> String {
    let mut out = String::new();
    let mut pending_dash = false;
    for c in title.chars() {
        let c = c.to_ascii_lowercase();
        if c.is_ascii_alphanumeric() {
            if pending_dash && !out.is_empty() {
                out.push('-');
            }
            pending_dash = false;
            out.push(c);
        } else {
            pending_dash = true;
        }
    }
    let mut slug: String = out.chars().take(MAX_LEN).collect();
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        "task".to_owned()
    } else {
        slug
    }
}

/// The slug with the number `n`: the slug itself for 1, and `-2`, `-3` and so on after it. The
/// result stays within [`MAX_LEN`].
pub fn numbered(slug: &str, n: u32) -> String {
    if n <= 1 {
        return slug.to_owned();
    }
    let suffix = format!("-{n}");
    let keep = MAX_LEN - suffix.len();
    let mut base: String = slug.chars().take(keep).collect();
    while base.ends_with('-') {
        base.pop();
    }
    format!("{base}{suffix}")
}

/// The slug, or the slug with `-2`, `-3` and so on until `taken` says no.
pub fn unique(slug: &str, taken: impl Fn(&str) -> bool) -> String {
    (1..)
        .map(|n| numbered(slug, n))
        .find(|candidate| !taken(candidate))
        .unwrap_or_else(|| slug.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_title_becomes_a_slug() {
        assert_eq!(slugify("Fix the login bug!"), "fix-the-login-bug");
        assert_eq!(slugify("  --Add  OAuth / SSO--  "), "add-oauth-sso");
        assert_eq!(slugify("Ünïcode only: 日本"), "n-code-only");
        assert_eq!(slugify("***"), "task");
        assert_eq!(slugify(""), "task");
    }

    #[test]
    fn a_long_title_is_cut_without_a_trailing_hyphen() {
        let slug = slugify(&"word ".repeat(30));
        assert!(slug.len() <= MAX_LEN, "{slug}");
        assert!(!slug.ends_with('-'));
    }

    #[test]
    fn a_collision_gets_a_number() {
        let taken = ["fix-bug", "fix-bug-2"];
        assert_eq!(unique("fix-bug", |s| taken.contains(&s)), "fix-bug-3");
        assert_eq!(unique("other", |s| taken.contains(&s)), "other");
    }

    #[test]
    fn a_suffix_never_pushes_the_slug_past_the_limit() {
        let long = "a".repeat(MAX_LEN);
        let taken = [long.as_str()];
        let next = unique(&long, |s| taken.contains(&s));
        assert_eq!(next.len(), MAX_LEN);
        assert!(next.ends_with("-2"));
    }
}
