// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    diag,
    diagnostics::Diagnostic,
    editions::{create_feature_error, Edition, FeatureGate},
    parser::syntax::make_loc,
    shared::CompilationEnv,
    FileCommentMap, MatchedFileCommentMap,
};
use move_command_line_common::{character_sets::DisplayChar, files::FileHash};
use move_ir_types::location::Loc;
use std::fmt;

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tok {
    EOF,
    NumValue,
    NumTypedValue,
    ByteStringValue,
    Identifier,
    SyntaxIdentifier,
    Exclaim,
    ExclaimEqual,
    Percent,
    Amp,
    AmpAmp,
    AmpMut,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Star,
    Plus,
    Comma,
    Minus,
    Period,
    PeriodPeriod,
    Slash,
    Colon,
    ColonColon,
    Semicolon,
    Less,
    LessEqual,
    LessLess,
    Equal,
    EqualEqual,
    EqualEqualGreater,
    LessEqualEqualGreater,
    Greater,
    GreaterEqual,
    GreaterGreater,
    Caret,
    Abort,
    Acquires,
    As,
    Break,
    Continue,
    Copy,
    Else,
    False,
    If,
    Invariant,
    Let,
    Loop,
    Module,
    Move,
    Native,
    Public,
    Return,
    Spec,
    Struct,
    True,
    Use,
    While,
    LBrace,
    Pipe,
    PipePipe,
    RBrace,
    Fun,
    Const,
    Friend,
    NumSign,
    AtSign,
    RestrictedIdentifier,
    Mut,
    Enum,
    Type,
    Match,
    BlockLabel,
    MinusGreater,
}

impl fmt::Display for Tok {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        use Tok::*;
        let s = match *self {
            EOF => "[end-of-file]",
            NumValue => "[Num]",
            NumTypedValue => "[NumTyped]",
            ByteStringValue => "[ByteString]",
            Identifier => "[Identifier]",
            SyntaxIdentifier => "[SyntaxIdentifier]",
            Exclaim => "!",
            ExclaimEqual => "!=",
            Percent => "%",
            Amp => "&",
            AmpAmp => "&&",
            AmpMut => "&mut",
            LParen => "(",
            RParen => ")",
            LBracket => "[",
            RBracket => "]",
            Star => "*",
            Plus => "+",
            Comma => ",",
            Minus => "-",
            Period => ".",
            PeriodPeriod => "..",
            Slash => "/",
            Colon => ":",
            ColonColon => "::",
            Semicolon => ";",
            Less => "<",
            LessEqual => "<=",
            LessLess => "<<",
            Equal => "=",
            EqualEqual => "==",
            EqualEqualGreater => "==>",
            LessEqualEqualGreater => "<==>",
            Greater => ">",
            GreaterEqual => ">=",
            GreaterGreater => ">>",
            Caret => "^",
            Abort => "abort",
            Acquires => "acquires",
            As => "as",
            Break => "break",
            Continue => "continue",
            Copy => "copy",
            Else => "else",
            False => "false",
            If => "if",
            Invariant => "invariant",
            Let => "let",
            Loop => "loop",
            Module => "module",
            Move => "move",
            Native => "native",
            Public => "public",
            Return => "return",
            Spec => "spec",
            Struct => "struct",
            True => "true",
            Use => "use",
            While => "while",
            LBrace => "{",
            Pipe => "|",
            PipePipe => "||",
            RBrace => "}",
            Fun => "fun",
            Const => "const",
            Friend => "friend",
            NumSign => "#",
            AtSign => "@",
            RestrictedIdentifier => "r#[Identifier]",
            Mut => "mut",
            Enum => "enum",
            Type => "type",
            Match => "match",
            BlockLabel => "'[Identifier]",
            MinusGreater => "->",
        };
        fmt::Display::fmt(s, formatter)
    }
}

pub struct Lexer<'input> {
    text: &'input str,
    file_hash: FileHash,
    edition: Edition,
    doc_comments: FileCommentMap,
    matched_doc_comments: MatchedFileCommentMap,
    prev_end: usize,
    cur_start: usize,
    cur_end: usize,
    token: Tok,
}

impl<'input> Lexer<'input> {
    pub fn new(text: &'input str, file_hash: FileHash, edition: Edition) -> Lexer<'input> {
        Lexer {
            text,
            file_hash,
            edition,
            doc_comments: FileCommentMap::new(),
            matched_doc_comments: MatchedFileCommentMap::new(),
            prev_end: 0,
            cur_start: 0,
            cur_end: 0,
            token: Tok::EOF,
        }
    }

    pub fn peek(&self) -> Tok {
        self.token
    }

    pub fn content(&self) -> &'input str {
        &self.text[self.cur_start..self.cur_end]
    }

    pub fn file_hash(&self) -> FileHash {
        self.file_hash
    }

    pub fn start_loc(&self) -> usize {
        self.cur_start
    }

    pub fn previous_end_loc(&self) -> usize {
        self.prev_end
    }

    pub fn current_token_loc(&self) -> Loc {
        make_loc(self.file_hash(), self.cur_start, self.cur_end)
    }

    pub fn edition(&self) -> Edition {
        self.edition
    }

    /// Strips line and block comments from input source, and collects documentation comments,
    /// putting them into a map indexed by the span of the comment region. Comments in the original
    /// source will be replaced by spaces, such that positions of source items stay unchanged.
    /// Block comments can be nested.
    ///
    /// Documentation comments are comments which start with
    /// `///` or `/**`, but not `////` or `/***`. The actually comment delimiters
    /// (`/// .. <newline>` and `/** .. */`) will be not included in extracted comment string. The
    /// span in the returned map, however, covers the whole region of the comment, including the
    /// delimiters.
    fn trim_whitespace_and_comments(
        &mut self,
        offset: usize,
    ) -> Result<&'input str, Box<Diagnostic>> {
        let mut text = &self.text[offset..];

        // A helper function to compute the index of the start of the given substring.
        let len = text.len();
        let get_offset = |substring: &str| offset + len - substring.len();

        // Loop until we find text that isn't whitespace, and that isn't part of
        // a multi-line or single-line comment.
        loop {
            // Trim the start whitespace characters.
            text = trim_start_whitespace(text);

            if text.starts_with("/*") {
                // Strip multi-line comments like '/* ... */' or '/** ... */'.
                // These can be nested, as in '/* /* ... */ */', so record the
                // start locations of each nested comment as a stack. The
                // boolean indicates whether it's a documentation comment.
                let mut locs: Vec<(usize, bool)> = vec![];
                loop {
                    text = text.trim_start_matches(|c: char| c != '/' && c != '*');
                    if text.is_empty() {
                        // We've reached the end of string while searching for a
                        // terminating '*/'.
                        let loc = *locs.last().unwrap();
                        // Highlight the '/**' if it's a documentation comment, or the '/*'
                        // otherwise.
                        let location =
                            make_loc(self.file_hash, loc.0, loc.0 + if loc.1 { 3 } else { 2 });
                        return Err(Box::new(diag!(
                            Syntax::InvalidDocComment,
                            (location, "Unclosed block comment"),
                        )));
                    } else if text.starts_with("/*") {
                        // We've found a (perhaps nested) multi-line comment.
                        let start = get_offset(text);
                        text = &text[2..];

                        // Check if this is a documentation comment: '/**', but not '/***'.
                        // A documentation comment cannot be nested within another comment.
                        let is_doc =
                            text.starts_with('*') && !text.starts_with("**") && locs.is_empty();

                        locs.push((start, is_doc));
                    } else if text.starts_with("*/") {
                        // We've found a multi-line comment terminator that ends
                        // our innermost nested comment.
                        let loc = locs.pop().unwrap();
                        text = &text[2..];

                        // If this was a documentation comment, record it in our map.
                        if loc.1 {
                            let end = get_offset(text);
                            self.doc_comments.insert(
                                (loc.0 as u32, end as u32),
                                self.text[(loc.0 + 3)..(end - 2)].to_string(),
                            );
                        }

                        // If this terminated our last comment, exit the loop.
                        if locs.is_empty() {
                            break;
                        }
                    } else {
                        // This is a solitary '/' or '*' that isn't part of any comment delimiter.
                        // Skip over it.
                        let c = text.chars().next().unwrap();
                        text = &text[c.len_utf8()..];
                    }
                }

                // Continue the loop immediately after the multi-line comment.
                // There may be whitespace or another comment following this one.
                continue;
            } else if text.starts_with("//") {
                let start = get_offset(text);
                let is_doc = text.starts_with("///") && !text.starts_with("////");
                text = text.trim_start_matches(|c: char| c != '\n');

                // If this was a documentation comment, record it in our map.
                if is_doc {
                    let end = get_offset(text);
                    let mut comment = &self.text[(start + 3)..end];
                    comment = comment.trim_end_matches(|c: char| c == '\r');

                    self.doc_comments
                        .insert((start as u32, end as u32), comment.to_string());
                }

                // Continue the loop on the following line, which may contain leading
                // whitespace or comments of its own.
                continue;
            }
            break;
        }
        Ok(text)
    }

    // Trim until reaching whitespace: space, tab, lf(\n) and crlf(\r\n).
    fn trim_until_whitespace(&self, offset: usize) -> &'input str {
        let mut text = &self.text[offset..];
        let mut iter = text.chars();
        while let Some(c) = iter.next() {
            if c == ' '
                || c == '\t'
                || c == '\n'
                || (c == '\r' && matches!(iter.next(), Some('\n')))
            {
                break;
            }
            text = &text[c.len_utf8()..];
        }
        text
    }

    // Look ahead to the next token after the current one and return it, and its starting offset,
    // without advancing the state of the lexer.
    pub fn lookahead(&mut self) -> Result<Tok, Box<Diagnostic>> {
        let text = self.trim_whitespace_and_comments(self.cur_end)?;
        let next_start = self.text.len() - text.len();
        let (result, _) = find_token(
            /* panic_mode */ false,
            self.file_hash,
            self.edition,
            text,
            next_start,
        );
        // unwrap safe because panic_mode is false
        result.map_err(|diag_opt| diag_opt.unwrap())
    }

    // Look ahead to the next two tokens after the current one and return them without advancing
    // the state of the lexer.
    pub fn lookahead2(&mut self) -> Result<(Tok, Tok), Box<Diagnostic>> {
        let text = self.trim_whitespace_and_comments(self.cur_end)?;
        let offset = self.text.len() - text.len();
        let (result, length) = find_token(
            /* panic_mode */ false,
            self.file_hash,
            self.edition,
            text,
            offset,
        );
        let first = result.map_err(|diag_opt| diag_opt.unwrap())?;
        let text2 = self.trim_whitespace_and_comments(offset + length)?;
        let offset2 = self.text.len() - text2.len();
        let (result2, _) = find_token(
            /* panic_mode */ false,
            self.file_hash,
            self.edition,
            text2,
            offset2,
        );
        let second = result2.map_err(|diag_opt| diag_opt.unwrap())?;
        Ok((first, second))
    }

    // Matches the doc comments after the last token (or the beginning of the file) to the position
    // of the current token. This moves the comments out of `doc_comments` and
    // into `matched_doc_comments`. At the end of parsing, if `doc_comments` is not empty, errors
    // for stale doc comments will be produced.
    //
    // Calling this function during parsing effectively marks a valid point for documentation
    // comments. The documentation comments are not stored in the AST, but can be retrieved by
    // using the start position of an item as an index into `matched_doc_comments`.
    pub fn match_doc_comments(&mut self) {
        let start = self.previous_end_loc() as u32;
        let end = self.cur_start as u32;
        let mut matched = vec![];
        let merged = self
            .doc_comments
            .range((start, start)..(end, end))
            .map(|(span, s)| {
                matched.push(*span);
                s.clone()
            })
            .collect::<Vec<String>>()
            .join("\n");
        for span in matched {
            self.doc_comments.remove(&span);
        }
        self.matched_doc_comments.insert(end, merged);
    }

    // At the end of parsing, checks whether there are any unmatched documentation comments,
    // producing errors if so. Otherwise returns a map from file position to associated
    // documentation.
    pub fn check_and_get_doc_comments(
        &mut self,
        env: &mut CompilationEnv,
    ) -> MatchedFileCommentMap {
        let msg = "Documentation comment cannot be matched to a language item";
        let diags = self
            .doc_comments
            .iter()
            .map(|((start, end), _)| {
                let loc = Loc::new(self.file_hash, *start, *end);
                diag!(Syntax::InvalidDocComment, (loc, msg))
            })
            .collect();
        env.add_diags(diags);
        std::mem::take(&mut self.matched_doc_comments)
    }

    /// Advance to the next token. This function will keep trying to advance the lexer until it
    /// actually finds a valid token, skipping over non-token text snippets if necessary (in the
    /// worst case, it will eventually encounter EOF). If parsing errors are encountered when
    /// skipping over non-tokens, the first diagnostic will be recorded and returned, so that it can
    /// be acted upon (if parsing needs to stop) or ignored (if parsing should proceed regardless).
    pub fn advance(&mut self) -> Result<(), Box<Diagnostic>> {
        let text_end = self.text.len();
        self.prev_end = self.cur_end;
        let mut err = None;
        // loop until the next valid token (which ultimately can be EOF) is found
        let token = loop {
            let mut cur_end = self.cur_end;
            // loop until the next text snippet which may contain a valid token is found)
            let text = loop {
                match self.trim_whitespace_and_comments(cur_end) {
                    Ok(t) => break t,
                    Err(diag) => {
                        // only report the first diag encountered
                        err = err.or(Some(diag));
                        // currently, this error can happen here if there is an unclosed block
                        // comment, in which case we advance to the next whitespace and try trimming
                        // again let
                        let trimmed = self.trim_until_whitespace(cur_end);
                        cur_end += trimmed.len();
                    }
                };
            };
            let new_start = self.text.len() - text.len();
            // panic_mode determines if a diag should be actually recorded in find_token (so that
            // only first one is recorded)
            let panic_mode = err.is_some();
            let (result, len) =
                find_token(panic_mode, self.file_hash, self.edition, text, new_start);
            self.cur_start = new_start;
            self.cur_end = std::cmp::min(self.cur_start + len, text_end);
            match result {
                Ok(token) => break token,
                Err(diag_opt) => {
                    // only report the first diag encountered
                    err = err.or(diag_opt);
                    if self.cur_end == text_end {
                        break Tok::EOF;
                    }
                }
            }
        };
        // regardless of whether an error was encountered (and diagnostic recorded) or not, the
        // token is advanced
        self.token = token;
        if let Some(err) = err {
            Err(err)
        } else {
            Ok(())
        }
    }

    // Replace the current token. The lexer will always match the longest token,
    // but sometimes the parser will prefer to replace it with a shorter one,
    // e.g., ">" instead of ">>".
    pub fn replace_token(&mut self, token: Tok, len: usize) {
        self.token = token;
        self.cur_end = self.cur_start + len;
    }
}

// Find the next token and its length without changing the state of the lexer.
fn find_token(
    panic_mode: bool,
    file_hash: FileHash,
    edition: Edition,
    text: &str,
    start_offset: usize,
) -> (Result<Tok, Option<Box<Diagnostic>>>, usize) {
    macro_rules! maybe_diag {
        ( $($s:stmt);* ) => {{
            if panic_mode {
                None
            } else {
                Some({
                    $($s)*
                })
            }
        }};
    }
    let c: char = match text.chars().next() {
        Some(next_char) => next_char,
        None => {
            return (Ok(Tok::EOF), 0);
        }
    };
    let (res, len) = match c {
        '0'..='9' => {
            if text.starts_with("0x") && text.len() > 2 {
                let (tok, hex_len) = get_hex_number(&text[2..]);
                if hex_len == 0 {
                    // Fall back to treating this as a "0" token.
                    (Ok(Tok::NumValue), 1)
                } else {
                    (Ok(tok), 2 + hex_len)
                }
            } else {
                let (tok, len) = get_decimal_number(text);
                (Ok(tok), len)
            }
        }
        '`' => {
            let (is_valid, len) = if (text.len() > 1)
                && matches!(text[1..].chars().next(), Some('A'..='Z' | 'a'..='z' | '_'))
            {
                let sub = &text[1..];
                let len = get_name_len(sub);
                if !matches!(text[1 + len..].chars().next(), Some('`')) {
                    (false, len + 1)
                } else {
                    (true, len + 2)
                }
            } else {
                (false, 1)
            };
            if !is_valid {
                let diag = maybe_diag! {
                    let loc = make_loc(file_hash, start_offset, start_offset + len);
                    let msg = "Missing closing backtick (`) for restricted identifier escaping";
                    Box::new(diag!(Syntax::InvalidRestrictedIdentifier, (loc, msg)))
                };
                (Err(diag), len)
            } else {
                (Ok(Tok::RestrictedIdentifier), len)
            }
        }
        '\'' if edition.supports(FeatureGate::BlockLabels) => {
            let (is_valid, len) = if (text.len() > 1)
                && matches!(text[1..].chars().next(), Some('A'..='Z' | 'a'..='z' | '_'))
            {
                let sub = &text[1..];
                let len = get_name_len(sub);
                (true, len + 1)
            } else {
                (false, 1)
            };
            if text[len..].starts_with('\'') {
                let diag = maybe_diag! {
                    let loc = make_loc(file_hash, start_offset, start_offset + len + 1);
                    let msg = "Single-quote (') may only prefix control flow labels";
                    let mut diag = diag!(Syntax::UnexpectedToken, (loc, msg));
                    diag.add_note(
                        "Character literals are not supported, \
                        and string literals use double-quote (\")."
                    );
                    Box::new(diag)
                };
                (Err(diag), len)
            } else if !is_valid {
                let diag = maybe_diag! {
                    let loc = make_loc(file_hash, start_offset, start_offset + len);
                    let msg = "Invalid control flow label";
                    Box::new(diag!(Syntax::UnexpectedToken, (loc, msg)))
                };
                (Err(diag), len)
            } else {
                (Ok(Tok::BlockLabel), len)
            }
        }
        '\'' => {
            let (is_valid, len) = if (text.len() > 1)
                && matches!(text[1..].chars().next(), Some('A'..='Z' | 'a'..='z' | '_'))
            {
                let sub = &text[1..];
                let len = get_name_len(sub);
                (true, len + 1)
            } else {
                (false, 1)
            };
            let rest_text = &text[len..];
            if rest_text.starts_with('\'') {
                let diag = maybe_diag! {
                    let loc = make_loc(file_hash, start_offset, start_offset + len + 1);
                    let msg = "Charater literals are not supported";
                    let mut diag = diag!(Syntax::UnexpectedToken, (loc, msg));
                    diag.add_note("String literals use double-quote (\").");
                    Box::new(diag)
                };
                (Err(diag), len)
            } else if is_valid && (rest_text.starts_with(':') || rest_text.starts_with(" {")) {
                let diag = maybe_diag! {
                    let loc = make_loc(file_hash, start_offset, start_offset + len);
                    Box::new(create_feature_error(edition, FeatureGate::BlockLabels, loc))
                };
                (Err(diag), len)
            } else {
                let diag = maybe_diag! {
                    let loc = make_loc(file_hash, start_offset, start_offset + len);
                    let msg = "Unexpected character (')";
                    Box::new(diag!(Syntax::InvalidCharacter, (loc, msg)))
                };
                (Err(diag), len)
            }
        }

        'A'..='Z' | 'a'..='z' | '_' => {
            let is_hex = text.starts_with("x\"");
            if is_hex || text.starts_with("b\"") {
                let line = &text.lines().next().unwrap()[2..];
                match get_string_len(line) {
                    Some(last_quote) => (Ok(Tok::ByteStringValue), 2 + last_quote + 1),
                    None => {
                        let diag = maybe_diag! {
                            let loc =
                                make_loc(file_hash, start_offset, start_offset + line.len() + 2);
                            Box::new(diag!(
                                if is_hex {
                                    Syntax::InvalidHexString
                                } else {
                                    Syntax::InvalidByteString
                                },
                                (loc, "Missing closing quote (\") after byte string")
                            ))
                        };
                        (Err(diag), line.len() + 2)
                    }
                }
            } else {
                let len = get_name_len(text);
                (Ok(get_name_token(edition, &text[..len])), len)
            }
        }
        '$' => {
            if text.len() > 1 && text[1..].starts_with(|c| matches!(c,'A'..='Z' | 'a'..='z' | '_'))
            {
                let len = get_name_len(&text[1..]);
                (Ok(Tok::SyntaxIdentifier), len + 1)
            } else {
                let loc = make_loc(file_hash, start_offset, start_offset);
                let diag = maybe_diag! { Box::new(diag!(
                    Syntax::UnexpectedToken,
                    (loc, "Expected an identifier following '$', e.g. '$x'"),
                )) };
                (Err(diag), 1)
            }
        }
        '&' => {
            if text.starts_with("&mut ") {
                (Ok(Tok::AmpMut), 5)
            } else if text.starts_with("&&") {
                (Ok(Tok::AmpAmp), 2)
            } else {
                (Ok(Tok::Amp), 1)
            }
        }
        '|' => {
            if text.starts_with("||") {
                (Ok(Tok::PipePipe), 2)
            } else {
                (Ok(Tok::Pipe), 1)
            }
        }
        '=' => {
            if text.starts_with("==>") {
                (Ok(Tok::EqualEqualGreater), 3)
            } else if text.starts_with("==") {
                (Ok(Tok::EqualEqual), 2)
            } else {
                (Ok(Tok::Equal), 1)
            }
        }
        '!' => {
            if text.starts_with("!=") {
                (Ok(Tok::ExclaimEqual), 2)
            } else {
                (Ok(Tok::Exclaim), 1)
            }
        }
        '<' => {
            if text.starts_with("<==>") {
                (Ok(Tok::LessEqualEqualGreater), 4)
            } else if text.starts_with("<=") {
                (Ok(Tok::LessEqual), 2)
            } else if text.starts_with("<<") {
                (Ok(Tok::LessLess), 2)
            } else {
                (Ok(Tok::Less), 1)
            }
        }
        '>' => {
            if text.starts_with(">=") {
                (Ok(Tok::GreaterEqual), 2)
            } else if text.starts_with(">>") {
                (Ok(Tok::GreaterGreater), 2)
            } else {
                (Ok(Tok::Greater), 1)
            }
        }
        ':' => {
            if text.starts_with("::") {
                (Ok(Tok::ColonColon), 2)
            } else {
                (Ok(Tok::Colon), 1)
            }
        }
        '.' => {
            if text.starts_with("..") {
                (Ok(Tok::PeriodPeriod), 2)
            } else {
                (Ok(Tok::Period), 1)
            }
        }
        '-' => {
            if text.starts_with("->") {
                (Ok(Tok::MinusGreater), 2)
            } else {
                (Ok(Tok::Minus), 1)
            }
        }
        '%' => (Ok(Tok::Percent), 1),
        '(' => (Ok(Tok::LParen), 1),
        ')' => (Ok(Tok::RParen), 1),
        '[' => (Ok(Tok::LBracket), 1),
        ']' => (Ok(Tok::RBracket), 1),
        '*' => (Ok(Tok::Star), 1),
        '+' => (Ok(Tok::Plus), 1),
        ',' => (Ok(Tok::Comma), 1),
        '/' => (Ok(Tok::Slash), 1),
        ';' => (Ok(Tok::Semicolon), 1),
        '^' => (Ok(Tok::Caret), 1),
        '{' => (Ok(Tok::LBrace), 1),
        '}' => (Ok(Tok::RBrace), 1),
        '#' => (Ok(Tok::NumSign), 1),
        '@' => (Ok(Tok::AtSign), 1),
        c => {
            let diag = maybe_diag! {
                let loc = make_loc(file_hash, start_offset, start_offset);
                Box::new(diag!(
                    Syntax::InvalidCharacter,
                    (loc, format!("Unexpected character: '{}'", DisplayChar(c),))
                ))
            };
            (Err(diag), c.len_utf8())
        }
    };
    (res, len)
}

// Return the length of the substring matching [a-zA-Z0-9_]. Note that
// this does not do any special check for whether the first character
// starts with a number, so the caller is responsible for any additional
// checks on the first character.
fn get_name_len(text: &str) -> usize {
    text.chars()
        .position(|c| !matches!(c, 'a'..='z' | 'A'..='Z' | '_' | '0'..='9'))
        .unwrap_or(text.len())
}

fn get_decimal_number(text: &str) -> (Tok, usize) {
    let num_text_len = text
        .chars()
        .position(|c| !matches!(c, '0'..='9' | '_'))
        .unwrap_or(text.len());
    get_number_maybe_with_suffix(text, num_text_len)
}

// Return the length of the substring containing characters in [0-9a-fA-F].
fn get_hex_number(text: &str) -> (Tok, usize) {
    let num_text_len = text
        .find(|c| !matches!(c, 'a'..='f' | 'A'..='F' | '0'..='9'| '_'))
        .unwrap_or(text.len());
    get_number_maybe_with_suffix(text, num_text_len)
}

// Given the text for a number literal and the length for the characters that match to the number
// portion, checks for a typed suffix.
fn get_number_maybe_with_suffix(text: &str, num_text_len: usize) -> (Tok, usize) {
    let rest = &text[num_text_len..];
    if rest.starts_with("u8") {
        (Tok::NumTypedValue, num_text_len + 2)
    } else if rest.starts_with("u64") || rest.starts_with("u16") || rest.starts_with("u32") {
        (Tok::NumTypedValue, num_text_len + 3)
    } else if rest.starts_with("u128") || rest.starts_with("u256") {
        (Tok::NumTypedValue, num_text_len + 4)
    } else {
        // No typed suffix
        (Tok::NumValue, num_text_len)
    }
}

// Return the length of the quoted string, or None if there is no closing quote.
fn get_string_len(text: &str) -> Option<usize> {
    let mut pos = 0;
    let mut iter = text.chars();
    while let Some(chr) = iter.next() {
        if chr == '\\' {
            // Skip over the escaped character (e.g., a quote or another backslash)
            if iter.next().is_some() {
                pos += 1;
            }
        } else if chr == '"' {
            return Some(pos);
        }
        pos += chr.len_utf8();
    }
    None
}

fn get_name_token(edition: Edition, name: &str) -> Tok {
    match name {
        "abort" => Tok::Abort,
        "acquires" => Tok::Acquires,
        "as" => Tok::As,
        "break" => Tok::Break,
        "const" => Tok::Const,
        "continue" => Tok::Continue,
        "copy" => Tok::Copy,
        "else" => Tok::Else,
        "false" => Tok::False,
        "fun" => Tok::Fun,
        "friend" => Tok::Friend,
        "if" => Tok::If,
        "invariant" => Tok::Invariant,
        "let" => Tok::Let,
        "loop" => Tok::Loop,
        "module" => Tok::Module,
        "move" => Tok::Move,
        "native" => Tok::Native,
        "public" => Tok::Public,
        "return" => Tok::Return,
        "spec" => Tok::Spec,
        "struct" => Tok::Struct,
        "true" => Tok::True,
        "use" => Tok::Use,
        "while" => Tok::While,
        _ if edition.supports(FeatureGate::Move2024Keywords) => match name {
            "mut" => Tok::Mut,
            "enum" => Tok::Enum,
            "type" => Tok::Type,
            "match" => Tok::Match,
            _ => Tok::Identifier,
        },
        _ => Tok::Identifier,
    }
}

// Trim the start whitespace characters, include: space, tab, lf(\n) and crlf(\r\n).
fn trim_start_whitespace(text: &str) -> &str {
    let mut pos = 0;
    let mut iter = text.chars();

    while let Some(chr) = iter.next() {
        match chr {
            ' ' | '\t' | '\n' => pos += 1,
            '\r' if matches!(iter.next(), Some('\n')) => pos += 2,
            _ => break,
        };
    }

    &text[pos..]
}

#[cfg(test)]
mod tests {
    use super::trim_start_whitespace;

    #[test]
    fn test_trim_start_whitespace() {
        assert_eq!(trim_start_whitespace("\r"), "\r");
        assert_eq!(trim_start_whitespace("\rxxx"), "\rxxx");
        assert_eq!(trim_start_whitespace("\t\rxxx"), "\rxxx");
        assert_eq!(trim_start_whitespace("\r\n\rxxx"), "\rxxx");

        assert_eq!(trim_start_whitespace("\n"), "");
        assert_eq!(trim_start_whitespace("\r\n"), "");
        assert_eq!(trim_start_whitespace("\t"), "");
        assert_eq!(trim_start_whitespace(" "), "");

        assert_eq!(trim_start_whitespace("\nxxx"), "xxx");
        assert_eq!(trim_start_whitespace("\r\nxxx"), "xxx");
        assert_eq!(trim_start_whitespace("\txxx"), "xxx");
        assert_eq!(trim_start_whitespace(" xxx"), "xxx");

        assert_eq!(trim_start_whitespace(" \r\n"), "");
        assert_eq!(trim_start_whitespace("\t\r\n"), "");
        assert_eq!(trim_start_whitespace("\n\r\n"), "");
        assert_eq!(trim_start_whitespace("\r\n "), "");
        assert_eq!(trim_start_whitespace("\r\n\t"), "");
        assert_eq!(trim_start_whitespace("\r\n\n"), "");

        assert_eq!(trim_start_whitespace(" \r\nxxx"), "xxx");
        assert_eq!(trim_start_whitespace("\t\r\nxxx"), "xxx");
        assert_eq!(trim_start_whitespace("\n\r\nxxx"), "xxx");
        assert_eq!(trim_start_whitespace("\r\n xxx"), "xxx");
        assert_eq!(trim_start_whitespace("\r\n\txxx"), "xxx");
        assert_eq!(trim_start_whitespace("\r\n\nxxx"), "xxx");

        assert_eq!(trim_start_whitespace(" \r\n\r\n"), "");
        assert_eq!(trim_start_whitespace("\r\n \t\n"), "");

        assert_eq!(trim_start_whitespace(" \r\n\r\nxxx"), "xxx");
        assert_eq!(trim_start_whitespace("\r\n \t\nxxx"), "xxx");

        assert_eq!(trim_start_whitespace(" \r\n\r\nxxx\n"), "xxx\n");
        assert_eq!(trim_start_whitespace("\r\n \t\nxxx\r\n"), "xxx\r\n");
        assert_eq!(trim_start_whitespace("\r\n\u{A0}\n"), "\u{A0}\n");
        assert_eq!(trim_start_whitespace("\r\n\u{A0}\n"), "\u{A0}\n");
        assert_eq!(trim_start_whitespace("\t  \u{0085}\n"), "\u{0085}\n")
    }
}
