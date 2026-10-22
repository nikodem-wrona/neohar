use super::*;

fn words(src: &str) -> Vec<String> {
    lex(src)
        .tokens
        .into_iter()
        .filter_map(|token| match token {
            Token::Word(word) => Some(word.text),
            _ => None,
        })
        .collect()
}

fn ops(src: &str) -> Vec<Op> {
    lex(src)
        .tokens
        .into_iter()
        .filter_map(|token| match token {
            Token::Op(op, _) => Some(op),
            _ => None,
        })
        .collect()
}

#[test]
fn words_split_at_white_space() {
    assert_eq!(words("git commit -m msg"), ["git", "commit", "-m", "msg"]);
    assert_eq!(words("  a\tb  "), ["a", "b"]);
    assert!(words("").is_empty());
}

#[test]
fn quotes_group_and_leave_the_text() {
    assert_eq!(
        words(r#"echo "a b" 'c d' e\ f"#),
        ["echo", "a b", "c d", "e f"]
    );
    assert_eq!(
        words(r#"echo "it's" 'say "hi"'"#),
        ["echo", "it's", "say \"hi\""]
    );
    assert_eq!(words(r#"echo a"b"c"#), ["echo", "abc"]);
    assert_eq!(
        words(r#"echo "a\"b" "c\\d" "e\nf""#),
        ["echo", "a\"b", "c\\d", "e\\nf"]
    );
    assert_eq!(words("echo $'a\\'b'"), ["echo", "a'b"]);
}

#[test]
fn a_quoted_word_is_flagged() {
    let tokens = lex("echo 'x' y").tokens;
    let quoted: Vec<bool> = tokens
        .iter()
        .filter_map(|t| match t {
            Token::Word(w) => Some(w.quoted),
            _ => None,
        })
        .collect();
    assert_eq!(quoted, [false, true, false]);
}

#[test]
fn operators_split_commands() {
    assert_eq!(
        ops("a; b && c || d | e & f\ng"),
        [
            Op::Semi,
            Op::And,
            Op::Or,
            Op::Pipe,
            Op::Background,
            Op::Newline
        ]
    );
    assert_eq!(ops("a |& b"), [Op::Pipe]);
    assert_eq!(ops("(a; b)"), [Op::Open, Op::Semi, Op::Close]);
    // An operator needs no space around it.
    assert_eq!(words("a&&b;c"), ["a", "b", "c"]);
}

#[test]
fn redirections_have_a_target_word() {
    let lexed = lex("cmd > out.txt 2>&1 >> log < in");
    let redirects: Vec<&str> = lexed
        .tokens
        .iter()
        .filter_map(|t| match t {
            Token::Redirect { op, .. } => Some(op.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(redirects, [">", "2>&", ">>", "<"]);
    assert_eq!(words("cmd > out.txt 2>&1"), ["cmd", "out.txt", "1"]);
    let both = lex("cmd &> all.txt");
    assert!(
        both.tokens
            .iter()
            .any(|t| matches!(t, Token::Redirect { op, .. } if op == "&>"))
    );
}

#[test]
fn a_digit_before_a_redirection_is_a_file_descriptor() {
    assert_eq!(words("cmd 2>/dev/null"), ["cmd", "/dev/null"]);
    // A digit word that is not before a redirection stays a word.
    assert_eq!(words("sleep 2 > out"), ["sleep", "2", "out"]);
}

#[test]
fn substitutions_are_found_and_the_word_is_dynamic() {
    let lexed = lex("echo $(rm -rf x) `ls` \"$(date)\" <(cat f)");
    assert_eq!(lexed.nested, ["rm -rf x", "ls", "date", "cat f"]);
    assert!(!lexed.open);
    for token in &lexed.tokens {
        if let Token::Word(word) = token
            && word.text != "echo"
        {
            assert!(word.dynamic, "{word:?}");
        }
    }
}

#[test]
fn nested_substitutions_keep_their_parentheses_in_balance() {
    let lexed = lex("echo $(echo $(echo ')') \"a)b\")");
    assert_eq!(lexed.nested.len(), 1);
    assert_eq!(lexed.nested[0], "echo $(echo ')') \"a)b\"");
}

#[test]
fn single_quotes_hide_a_substitution() {
    let lexed = lex("echo '$(rm -rf /)'");
    assert!(lexed.nested.is_empty());
}

#[test]
fn variables_and_patterns_make_a_word_dynamic() {
    for src in [
        "echo $HOME",
        "echo ${HOME}/x",
        "echo *.rs",
        "echo a?c",
        "echo $1",
        "echo $((1+2))",
    ] {
        let tokens = lex(src).tokens;
        let Some(Token::Word(word)) = tokens.get(1) else {
            panic!("{src}")
        };
        assert!(word.dynamic, "{src}");
    }
    let tokens = lex("echo '*' \"$\"").tokens;
    let Some(Token::Word(star)) = tokens.get(1) else {
        panic!("word")
    };
    assert!(!star.dynamic);
}

#[test]
fn an_open_quote_or_substitution_is_flagged() {
    assert!(lex("echo 'abc").open);
    assert!(lex("echo \"abc").open);
    assert!(lex("echo $(abc").open);
    assert!(lex("echo `abc").open);
    assert!(!lex("echo 'abc'").open);
}

#[test]
fn comments_end_at_the_line_end() {
    let lexed = lex("ls # list\npwd");
    assert!(lexed.tokens.iter().any(|t| matches!(t, Token::Comment(_))));
    assert_eq!(words("ls # list\npwd"), ["ls", "pwd"]);
    // A hash inside a word is not a comment.
    assert_eq!(words("echo a#b"), ["echo", "a#b"]);
}

#[test]
fn a_here_document_body_is_not_a_command_but_its_substitutions_are() {
    let src = "cat <<EOF\nrm -rf /\nvalue: $(whoami)\nEOF\nls";
    let lexed = lex(src);
    assert_eq!(lexed.nested, ["whoami"]);
    assert_eq!(words(src), ["cat", "EOF", "ls"]);
    // A quoted delimiter turns the expansion off.
    let quoted = lex("cat <<'EOF'\n$(whoami)\nEOF\n");
    assert!(quoted.nested.is_empty());
    // A tab-stripped delimiter.
    let tabs = lex("cat <<-EOF\n\t$(id)\n\tEOF\npwd");
    assert_eq!(tabs.nested, ["id"]);
    assert!(words("cat <<-EOF\n\t$(id)\n\tEOF\npwd").ends_with(&["pwd".to_owned()]));
}

#[test]
fn a_line_continuation_joins_lines() {
    assert_eq!(words("echo a \\\n b"), ["echo", "a", "b"]);
}

#[test]
fn ranges_point_into_the_source() {
    let src = "git  push --force";
    for token in lex(src).tokens {
        if let Token::Word(word) = token {
            assert_eq!(&src[word.range.clone()], word.text);
        }
    }
}

#[test]
fn multibyte_text_does_not_break_the_reader() {
    assert_eq!(words("echo 'héllo' wörld"), ["echo", "héllo", "wörld"]);
    assert_eq!(words("echo 日本語 > файл"), ["echo", "日本語", "файл"]);
}
