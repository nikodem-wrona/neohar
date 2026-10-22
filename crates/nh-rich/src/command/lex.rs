//! A reader for the shell syntax of one command line.
//!
//! It is not a shell. It splits a line into words, operators and redirections, takes the quotes out
//! of the words, and finds the commands that the line runs inside `$(…)`, backticks, `<(…)` and the
//! body of a here-document. The risk rules need exactly this much. A line that the reader does not
//! understand is flagged, and the rules treat it with care.

use std::ops::Range;

/// A word of a command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Word {
    /// The text with the quotes and the escapes taken out.
    pub text: String,
    pub range: Range<usize>,
    /// Part of the word was in quotes.
    pub quoted: bool,
    /// The value is not known before the shell runs: a variable, a substitution or a pattern.
    pub dynamic: bool,
    /// The command lines inside the substitutions of this word.
    pub subs: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Op {
    /// `;`
    Semi,
    /// `&&`
    And,
    /// `||`
    Or,
    /// `|` and `|&`
    Pipe,
    /// `&`
    Background,
    Newline,
    /// `(`
    Open,
    /// `)`
    Close,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Token {
    Word(Word),
    Op(Op, Range<usize>),
    /// `>`, `>>`, `2>`, `&>`, `<`, `<<`: the next word is the target.
    Redirect {
        op: String,
        range: Range<usize>,
    },
    Comment(Range<usize>),
}

#[derive(Debug, Default)]
pub(super) struct Lexed {
    pub tokens: Vec<Token>,
    /// The text of each `$(…)`, backtick pair and `<(…)`, and the substitutions in the body of a
    /// here-document. Each is a command line.
    pub nested: Vec<String>,
    /// A quote or a substitution has no end.
    pub open: bool,
}

struct Heredoc {
    delimiter: String,
    strip_tabs: bool,
    /// The delimiter was quoted, so the body has no expansion.
    literal: bool,
}

struct Lexer<'a> {
    src: &'a str,
    i: usize,
    out: Lexed,
    heredocs: Vec<Heredoc>,
}

pub(super) fn lex(src: &str) -> Lexed {
    let mut lexer = Lexer {
        src,
        i: 0,
        out: Lexed::default(),
        heredocs: Vec::new(),
    };
    lexer.run();
    lexer.out
}

fn is_break(ch: char) -> bool {
    matches!(
        ch,
        ' ' | '\t' | '\n' | ';' | '&' | '|' | '(' | ')' | '<' | '>'
    )
}

impl Lexer<'_> {
    fn peek(&self) -> Option<char> {
        self.src[self.i..].chars().next()
    }

    fn peek_at(&self, offset: usize) -> Option<char> {
        self.src.get(self.i + offset..)?.chars().next()
    }

    fn run(&mut self) {
        while let Some(ch) = self.peek() {
            let start = self.i;
            match ch {
                ' ' | '\t' | '\r' => self.i += 1,
                '\\' if self.peek_at(1) == Some('\n') => self.i += 2,
                '\n' => {
                    self.i += 1;
                    self.out.tokens.push(Token::Op(Op::Newline, start..self.i));
                    self.read_heredocs();
                }
                '#' => {
                    let end = self.src[start..]
                        .find('\n')
                        .map_or(self.src.len(), |n| start + n);
                    self.i = end;
                    self.out.tokens.push(Token::Comment(start..end));
                }
                ';' => {
                    self.i += if self.peek_at(1) == Some(';') { 2 } else { 1 };
                    self.out.tokens.push(Token::Op(Op::Semi, start..self.i));
                }
                '&' => match self.peek_at(1) {
                    Some('&') => {
                        self.i += 2;
                        self.out.tokens.push(Token::Op(Op::And, start..self.i));
                    }
                    Some('>') => {
                        self.i += if self.peek_at(2) == Some('>') { 3 } else { 2 };
                        self.push_redirect(start);
                    }
                    _ => {
                        self.i += 1;
                        self.out
                            .tokens
                            .push(Token::Op(Op::Background, start..self.i));
                    }
                },
                '|' => {
                    self.i += match self.peek_at(1) {
                        Some('|') => {
                            self.i += 2;
                            self.out.tokens.push(Token::Op(Op::Or, start..self.i));
                            continue;
                        }
                        Some('&') => 2,
                        _ => 1,
                    };
                    self.out.tokens.push(Token::Op(Op::Pipe, start..self.i));
                }
                '(' => {
                    self.i += 1;
                    self.out.tokens.push(Token::Op(Op::Open, start..self.i));
                }
                ')' => {
                    self.i += 1;
                    self.out.tokens.push(Token::Op(Op::Close, start..self.i));
                }
                '<' | '>' if self.peek_at(1) == Some('(') => self.read_word(),
                '<' | '>' => self.read_redirect(start),
                digit if digit.is_ascii_digit() && self.fd_before_redirect() => {
                    while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                        self.i += 1;
                    }
                    self.read_redirect(start);
                }
                _ => self.read_word(),
            }
        }
    }

    /// The digits at the cursor are a file descriptor when a redirection follows them.
    fn fd_before_redirect(&self) -> bool {
        let rest = &self.src[self.i..];
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        matches!(rest.as_bytes().get(digits), Some(b'<' | b'>'))
    }

    fn push_redirect(&mut self, start: usize) {
        let op = self.src[start..self.i].to_owned();
        self.out.tokens.push(Token::Redirect {
            op,
            range: start..self.i,
        });
    }

    /// A redirection operator at the cursor, after any file descriptor. A here-document also reads
    /// its delimiter.
    fn read_redirect(&mut self, start: usize) {
        let Some(first) = self.peek() else { return };
        self.i += 1;
        let mut heredoc = false;
        match (first, self.peek()) {
            ('>', Some('>' | '|' | '&')) | ('<', Some('&' | '>')) => self.i += 1,
            ('<', Some('<')) => {
                self.i += 1;
                match self.peek() {
                    Some('<') => self.i += 1,
                    Some('-') => {
                        self.i += 1;
                        heredoc = true;
                    }
                    _ => heredoc = true,
                }
            }
            _ => {}
        }
        self.push_redirect(start);
        if !heredoc {
            return;
        }
        let strip_tabs = self.src[start..self.i].ends_with('-');
        while matches!(self.peek(), Some(' ' | '\t')) {
            self.i += 1;
        }
        let before = self.out.tokens.len();
        self.read_word();
        if let Some(Token::Word(word)) = self.out.tokens.get(before) {
            self.heredocs.push(Heredoc {
                delimiter: word.text.clone(),
                strip_tabs,
                literal: word.quoted,
            });
        }
    }

    /// After a line end: the bodies of the here-documents that the line started.
    fn read_heredocs(&mut self) {
        for heredoc in std::mem::take(&mut self.heredocs) {
            let body_start = self.i;
            let mut body_end = self.src.len();
            let mut resume = self.src.len();
            let mut at = self.i;
            while at < self.src.len() {
                let line_end = self.src[at..].find('\n').map_or(self.src.len(), |n| at + n);
                let line = self.src[at..line_end].trim_end_matches('\r');
                let line = if heredoc.strip_tabs {
                    line.trim_start_matches('\t')
                } else {
                    line
                };
                if line == heredoc.delimiter {
                    body_end = at;
                    resume = (line_end + 1).min(self.src.len());
                    break;
                }
                at = line_end + 1;
            }
            if !heredoc.literal {
                let body = &self.src[body_start..body_end];
                self.scan_body(body);
            }
            self.i = resume;
        }
    }

    /// Finds the substitutions in text that the shell expands, such as a here-document.
    fn scan_body(&mut self, body: &str) {
        let mut inner = Lexer {
            src: body,
            i: 0,
            out: Lexed::default(),
            heredocs: Vec::new(),
        };
        while let Some(ch) = inner.peek() {
            match ch {
                '\\' => inner.i += 1 + inner.peek_at(1).map_or(0, char::len_utf8),
                '$' if inner.peek_at(1) == Some('(') && inner.peek_at(2) != Some('(') => {
                    inner.substitution();
                }
                '`' => {
                    inner.backticks();
                }
                other => inner.i += other.len_utf8(),
            }
        }
        self.out.nested.append(&mut inner.out.nested);
        self.out.open |= inner.out.open;
    }

    /// `$(…)` at the cursor. The text inside is a nested command line.
    fn substitution(&mut self) -> String {
        let start = self.i;
        self.i += 2;
        let inner_start = self.i;
        match self.find_close(self.i) {
            Some(close) => {
                self.out
                    .nested
                    .push(self.src[inner_start..close].to_owned());
                self.i = close + 1;
            }
            None => {
                self.out.nested.push(self.src[inner_start..].to_owned());
                self.out.open = true;
                self.i = self.src.len();
            }
        }
        self.src[start..self.i].to_owned()
    }

    /// `` `…` `` at the cursor.
    fn backticks(&mut self) -> String {
        let start = self.i;
        self.i += 1;
        let inner_start = self.i;
        let mut at = self.i;
        while at < self.src.len() {
            match self.src.as_bytes()[at] {
                b'\\' => at += 2,
                b'`' => {
                    self.out
                        .nested
                        .push(self.src[inner_start..at].replace("\\`", "`"));
                    self.i = at + 1;
                    return self.src[start..self.i].to_owned();
                }
                _ => at += 1,
            }
        }
        self.out.nested.push(self.src[inner_start..].to_owned());
        self.out.open = true;
        self.i = self.src.len();
        self.src[start..].to_owned()
    }

    /// The position of the `)` that closes the `$(` before `from`.
    fn find_close(&self, from: usize) -> Option<usize> {
        let bytes = self.src.as_bytes();
        let mut depth = 1usize;
        let mut at = from;
        while at < bytes.len() {
            match bytes[at] {
                b'\\' => at += 1,
                b'\'' => {
                    at += 1;
                    while at < bytes.len() && bytes[at] != b'\'' {
                        at += 1;
                    }
                }
                b'"' => {
                    at += 1;
                    while at < bytes.len() && bytes[at] != b'"' {
                        if bytes[at] == b'\\' {
                            at += 1;
                        } else if bytes[at] == b'$' && bytes.get(at + 1) == Some(&b'(') {
                            at = self.find_close(at + 2)?;
                        }
                        at += 1;
                    }
                }
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(at);
                    }
                }
                _ => {}
            }
            at += 1;
        }
        None
    }

    fn read_word(&mut self) {
        let start = self.i;
        let nested_before = self.out.nested.len();
        let mut word = Word {
            text: String::new(),
            range: start..start,
            quoted: false,
            dynamic: false,
            subs: Vec::new(),
        };
        while let Some(ch) = self.peek() {
            match ch {
                '\'' => {
                    word.quoted = true;
                    self.i += 1;
                    let end = self.src[self.i..].find('\'');
                    match end {
                        Some(n) => {
                            word.text.push_str(&self.src[self.i..self.i + n]);
                            self.i += n + 1;
                        }
                        None => {
                            word.text.push_str(&self.src[self.i..]);
                            self.out.open = true;
                            self.i = self.src.len();
                        }
                    }
                }
                '"' => {
                    word.quoted = true;
                    self.i += 1;
                    self.read_double_quoted(&mut word);
                }
                '\\' => {
                    self.i += 1;
                    if let Some(next) = self.peek() {
                        if next != '\n' {
                            word.text.push(next);
                            word.quoted = true;
                        }
                        self.i += next.len_utf8();
                    }
                }
                '$' => self.read_dollar(&mut word),
                '`' => {
                    let text = self.backticks();
                    word.text.push_str(&text);
                    word.dynamic = true;
                }
                '<' | '>' if self.peek_at(1) == Some('(') && self.i == start => {
                    // Process substitution: the shell runs the command and gives a file name.
                    let kind = ch;
                    self.i += 1;
                    let text = self.substitution_from_paren();
                    word.text.push(kind);
                    word.text.push_str(&text);
                    word.dynamic = true;
                }
                '*' | '?' | '[' => {
                    word.dynamic = true;
                    word.text.push(ch);
                    self.i += 1;
                }
                other if is_break(other) => break,
                other => {
                    word.text.push(other);
                    self.i += other.len_utf8();
                }
            }
        }
        word.range = start..self.i;
        word.subs = self.out.nested[nested_before..].to_vec();
        if self.i > start {
            self.out.tokens.push(Token::Word(word));
        }
    }

    /// `(…)` of a process substitution at the cursor.
    fn substitution_from_paren(&mut self) -> String {
        let start = self.i;
        self.i += 1;
        let inner_start = self.i;
        match self.find_close(self.i) {
            Some(close) => {
                self.out
                    .nested
                    .push(self.src[inner_start..close].to_owned());
                self.i = close + 1;
            }
            None => {
                self.out.nested.push(self.src[inner_start..].to_owned());
                self.out.open = true;
                self.i = self.src.len();
            }
        }
        self.src[start..self.i].to_owned()
    }

    fn read_dollar(&mut self, word: &mut Word) {
        match self.peek_at(1) {
            Some('(') if self.peek_at(2) == Some('(') => {
                // Arithmetic: `$((1 + 2))`. No command inside, unless it has its own `$(`.
                let start = self.i;
                self.i += 3;
                while self.i < self.src.len() && !self.src[self.i..].starts_with("))") {
                    if self.src[self.i..].starts_with("$(") {
                        self.substitution();
                    } else {
                        self.i += self.peek().map_or(1, char::len_utf8);
                    }
                }
                self.i = (self.i + 2).min(self.src.len());
                word.text.push_str(&self.src[start..self.i]);
                word.dynamic = true;
            }
            Some('(') => {
                let text = self.substitution();
                word.text.push_str(&text);
                word.dynamic = true;
            }
            Some('{') => {
                let start = self.i;
                match self.src[self.i..].find('}') {
                    Some(n) => self.i += n + 1,
                    None => {
                        self.i = self.src.len();
                        self.out.open = true;
                    }
                }
                word.text.push_str(&self.src[start..self.i]);
                word.dynamic = true;
            }
            Some('\'') => {
                // `$'…'`: a string with escapes.
                self.i += 2;
                word.quoted = true;
                while let Some(ch) = self.peek() {
                    self.i += ch.len_utf8();
                    match ch {
                        '\\' => {
                            if let Some(next) = self.peek() {
                                word.text.push(next);
                                self.i += next.len_utf8();
                            }
                        }
                        '\'' => return,
                        other => word.text.push(other),
                    }
                }
                self.out.open = true;
            }
            Some(next)
                if next.is_ascii_alphanumeric()
                    || matches!(next, '_' | '@' | '*' | '#' | '?' | '$' | '!' | '-') =>
            {
                word.text.push('$');
                self.i += 1;
                if next.is_ascii_alphabetic() || next == '_' {
                    while let Some(c) = self.peek() {
                        if c.is_ascii_alphanumeric() || c == '_' {
                            word.text.push(c);
                            self.i += 1;
                        } else {
                            break;
                        }
                    }
                } else {
                    word.text.push(next);
                    self.i += next.len_utf8();
                }
                word.dynamic = true;
            }
            _ => {
                word.text.push('$');
                self.i += 1;
            }
        }
    }

    /// The inside of a double quote, up to the closing quote.
    fn read_double_quoted(&mut self, word: &mut Word) {
        while let Some(ch) = self.peek() {
            match ch {
                '"' => {
                    self.i += 1;
                    return;
                }
                '\\' => {
                    self.i += 1;
                    match self.peek() {
                        Some(next @ ('"' | '\\' | '$' | '`')) => {
                            word.text.push(next);
                            self.i += 1;
                        }
                        Some('\n') => self.i += 1,
                        Some(_) => word.text.push('\\'),
                        None => {}
                    }
                }
                '$' => self.read_dollar(word),
                '`' => {
                    let text = self.backticks();
                    word.text.push_str(&text);
                    word.dynamic = true;
                }
                other => {
                    word.text.push(other);
                    self.i += other.len_utf8();
                }
            }
        }
        self.out.open = true;
    }
}

#[cfg(test)]
mod tests;
