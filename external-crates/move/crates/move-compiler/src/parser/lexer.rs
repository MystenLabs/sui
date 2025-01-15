// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    diag,
    diagnostics::Diagnostic,
    editions::{create_feature_error, Edition, FeatureGate},
    parser::{syntax::make_loc, token_set::TokenSet},
};
use move_command_line_common::{character_sets::DisplayChar, files::FileHash};
use move_ir_types::location::Loc;
use std::{collections::BTreeSet, fmt};

// This should be replaced with std::mem::variant::count::<Tok>() if it ever comes out of nightly.
pub const TOK_COUNT: usize = 77;

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
    EqualGreater,
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
    For,
}

impl fmt::Display for Tok {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        use Tok::*;
        let s = match *self {
            EOF => "<End-Of-File>",
            NumValue => "<Number>",
            NumTypedValue => "<TypedNumber>",
            ByteStringValue => "<ByteString>",
            Identifier => "<Identifier>",
            SyntaxIdentifier => "$<Identifier>",
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
            Equal => "=",
            EqualEqual => "==",
            EqualEqualGreater => "==>",
            EqualGreater => "=>",
            LessEqualEqualGreater => "<==>",
            LessLess => "<<",
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
            RestrictedIdentifier => "r#<Identifier>",
            Mut => "mut",
            Enum => "enum",
            Type => "type",
            Match => "match",
            BlockLabel => "'<Identifier>",
            MinusGreater => "->",
            For => "for",
        };
        fmt::Display::fmt(s, formatter)
    }
}

pub struct Lexer<'input> {
    pub text: &'input str,
    file_hash: FileHash,
    edition: Edition,
    current_doc_comment: Option<(u32, u32, String)>,
    unmatched_doc_comments: Vec<(u32, u32, String)>,
    prev_end: usize,
    cur_start: usize,
    cur_end: usize,
    token: Tok,
    preceded_by_eol: bool, // last token was preceded by end-of-line
}

impl<'input> Lexer<'input> {
    pub fn new(text: &'input str, file_hash: FileHash, edition: Edition) -> Lexer<'input> {
        Lexer {
            text,
            file_hash,
            edition,
            current_doc_comment: None,
            unmatched_doc_comments: vec![],
            prev_end: 0,
            cur_start: 0,
            cur_end: 0,
            token: Tok::EOF,
            preceded_by_eol: false,
        }
    }

    pub fn peek(&self) -> Tok {
        self.token
    }

    pub fn remaining(&self) -> &'input str {
        &self.text[self.cur_start..]
    }

    pub fn at(&self, tok: Tok) -> bool {
        self.token == tok
    }

    pub fn at_any(&self, toks: &BTreeSet<Tok>) -> bool {
        toks.contains(&self.token)
    }

    pub fn at_set(&self, set: &TokenSet) -> bool {
        set.contains(self.token, self.content())
    }

    pub fn content(&self) -> &'input str {
        &self.text[self.cur_start..self.cur_end]
    }

    pub fn loc_contents(&self, loc: Loc) -> &'input str {
        &self.text[loc.start() as usize..loc.end() as usize]
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

    pub fn last_token_preceded_by_eol(&self) -> bool {
        self.preceded_by_eol
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
        track_doc_comments: bool,
    ) -> Result<(&'input str, bool), Box<Diagnostic>> {
        let mut trimmed_preceding_eol;
        let mut text = &self.text[offset..];

        // A helper function to compute the index of the start of the given substring.
        let len = text.len();
        let get_offset = |substring: &str| offset + len - substring.len();

        // Loop until we find text that isn't whitespace, and that isn't part of
        // a multi-line or single-line comment.
        loop {
            // Trim the start whitespace characters.
            (text, trimmed_preceding_eol) = trim_start_whitespace(text);

            if text.starts_with("/*") {
                // Continue the loop immediately after the multi-line comment.
                // There may be whitespace or another comment following this one.
                text = self.parse_block_comment(get_offset(text), track_doc_comments)?;
                continue;
            } else if text.starts_with("//") {
                let start = get_offset(text);
                let is_doc = text.starts_with("///") && !text.starts_with("////");
                text = text.trim_start_matches(|c: char| c != '\n');

                // If this was a documentation comment, append it to the current doc comment
                if track_doc_comments {
                    if is_doc {
                        let end = get_offset(text);
                        let mut comment = &self.text[(start + 3)..end];
                        comment = comment.trim_end_matches('\r');

                        self.append_current_doc_comment(start, end, comment);
                    } else {
                        self.advance_doc_comment();
                    }
                }
                // Continue the loop on the following line, which may contain leading
                // whitespace or comments of its own.
                continue;
            }
            break;
        }
        Ok((text, trimmed_preceding_eol))
    }

    fn parse_block_comment(
        &mut self,
        offset: usize,
        track_doc_comments: bool,
    ) -> Result<&'input str, Box<Diagnostic>> {
        struct CommentEntry {
            start: usize,
            is_doc_comment: bool,
        }

        let text = &self.text[offset..];

        // A helper function to compute the index of the start of the given substring.
        let len = text.len();
        let get_offset = |substring: &str| offset + len - substring.len();

        let block_doc_comment_start: &str = "/**";

        assert!(text.starts_with("/*"));
        let initial_entry = CommentEntry {
            start: get_offset(text),
            is_doc_comment: text.starts_with(block_doc_comment_start),
        };
        let mut comment_queue: Vec<CommentEntry> = vec![initial_entry];

        // This is a _rough_ apporximation which disregards doc comments in order to handle the
        // case where we have `/**/` or similar.
        let mut text = &text[2..];

        while let Some(comment) = comment_queue.pop() {
            text = text.trim_start_matches(|c: char| c != '/' && c != '*');
            if text.is_empty() {
                // We've reached the end of string while searching for a terminating '*/'.
                // Highlight the '/**' if it's a documentation comment, or the '/*' otherwise.
                let location = make_loc(
                    self.file_hash,
                    comment.start,
                    comment.start + if comment.is_doc_comment { 3 } else { 2 },
                );
                return Err(Box::new(diag!(
                    Syntax::InvalidDocComment,
                    (location, "Unclosed block comment"),
                )));
            };

            match &text[..2] {
                "*/" => {
                    let end = get_offset(text);
                    // only consider doc comments for the outermost block comment
                    // (and if `track_doc_comments`` is true)
                    if track_doc_comments && comment_queue.is_empty() {
                        // If the comment was not empty -- fuzzy ot handle `/**/`, which triggers the
                        // doc comment check but is not actually a doc comment.
                        if comment.is_doc_comment && comment.start + 3 < end {
                            self.append_current_doc_comment(
                                comment.start,
                                end,
                                &self.text[(comment.start + 3)..end],
                            );
                        } else {
                            self.advance_doc_comment();
                        }
                    }
                    text = &text[2..];
                }
                "/*" => {
                    comment_queue.push(comment);
                    let new_comment = CommentEntry {
                        start: get_offset(text),
                        is_doc_comment: text.starts_with(block_doc_comment_start),
                    };
                    comment_queue.push(new_comment);
                    text = &text[2..];
                }
                _ => {
                    // This is a solitary '/' or '*' that isn't part of any comment delimiter.
                    // Skip over it.
                    comment_queue.push(comment);
                    let c = text.chars().next().unwrap();
                    text = &text[c.len_utf8()..];
                }
            }
        }
        Ok(text)
    }

    fn advance_doc_comment(&mut self) {
        if let Some(c) = self.current_doc_comment.take() {
            self.unmatched_doc_comments.push(c)
        }
    }

    fn append_current_doc_comment(&mut self, start: usize, end: usize, comment: &str) {
        match self.current_doc_comment.as_mut() {
            None => {
                self.current_doc_comment = Some((start as u32, end as u32, comment.to_string()));
            }
            Some((_doc_start, doc_end, s)) => {
                *doc_end = end as u32;
                s.push('\n');
                s.push_str(comment);
            }
        }
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
        let (text, _) =
            self.trim_whitespace_and_comments(self.cur_end, /* track doc comments */ false)?;
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
        let (text, _) =
            self.trim_whitespace_and_comments(self.cur_end, /* track doc comments */ false)?;
        let offset = self.text.len() - text.len();
        let (result, length) = find_token(
            /* panic_mode */ false,
            self.file_hash,
            self.edition,
            text,
            offset,
        );
        let first = result.map_err(|diag_opt| diag_opt.unwrap())?;
        let (text2, _) = self
            .trim_whitespace_and_comments(offset + length, /* track doc comments */ false)?;
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

    // Takes the doc comment of the current token, if there is one. It modifies the lexer state to
    // and removes the doc comment, so this should be used just prior to `advance`, i.e. it should
    // not be used with peek or lookahead.
    pub fn take_doc_comment(&mut self) -> Option<(u32, u32, String)> {
        self.current_doc_comment.take()
    }

    // Restores the doc comment that was temporarily taken by `take_doc_comment`. This is used to
    // allow for tokens to intersperse a doc comment, like in the case of attributes `#[...]` where
    // a doc comment can continue with an attribute in the middle, e.g.
    // ```
    // /// This is a doc comment for 'fun foo'
    // #[attr]
    // /// This is a part of the same doc comment for 'fun foo'
    // fun foo() {}
    // ```
    pub fn restore_doc_comment(&mut self, restored_opt: Option<(u32, u32, String)>) {
        let Some((restored_start, restored_end, mut restored_comment)) = restored_opt else {
            return;
        };
        match self.current_doc_comment.as_mut() {
            None => {
                self.current_doc_comment = Some((restored_start, restored_end, restored_comment));
            }
            Some((doc_start, _doc_end, doc_comment)) => {
                *doc_start = restored_start;
                restored_comment.push('\n');
                restored_comment.push_str(doc_comment);
                *doc_comment = restored_comment;
            }
        }
    }

    // At the end of parsing, checks whether there are any unmatched documentation comments,
    // producing errors if so. Otherwise returns a map from file position to associated
    // documentation.
    pub fn take_unmatched_doc_comments(&mut self) -> Vec<(u32, u32, String)> {
        self.advance_doc_comment();
        std::mem::take(&mut self.unmatched_doc_comments)
    }

    /// Advance to the next token. This function will keep trying to advance the lexer until it
    /// actually finds a valid token, skipping over non-token text snippets if necessary (in the
    /// worst case, it will eventually encounter EOF). If parsing errors are encountered when
    /// skipping over non-tokens, the first diagnostic will be recorded and returned, so that it can
    /// be acted upon (if parsing needs to stop) or ignored (if parsing should proceed regardless).
    pub fn advance(&mut self) -> Result<(), Box<Diagnostic>> {
        self.advance_doc_comment();
        let text_end = self.text.len();
        self.prev_end = self.cur_end;
        let mut err = None;
        // loop until the next valid token (which ultimately can be EOF) is found
        let token = loop {
            let mut cur_end = self.cur_end;
            // loop until the next text snippet which may contain a valid token is found)
            let (text, trimmed_preceding_eol) = loop {
                match self.trim_whitespace_and_comments(cur_end, /* track doc comments */ true) {
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
            self.preceded_by_eol = trimmed_preceding_eol;
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
            } else if text.starts_with("=>") && edition.supports(FeatureGate::Enums) {
                (Ok(Tok::EqualGreater), 2)
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
            "for" => Tok::For,
            _ => Tok::Identifier,
        },
        _ => Tok::Identifier,
    }
}

// Trim the start whitespace characters, include: space, tab, lf(\n) and crlf(\r\n).
fn trim_start_whitespace(text: &str) -> (&str, bool) {
    let mut trimmed_eof = false;
    let mut pos = 0;
    let mut iter = text.chars();

    while let Some(chr) = iter.next() {
        match chr {
            '\n' => {
                pos += 1;
                trimmed_eof = true;
            }
            ' ' | '\t' => pos += 1,
            '\r' if matches!(iter.next(), Some('\n')) => {
                pos += 2;
                trimmed_eof = true;
            }
            _ => break,
        };
    }

    (&text[pos..], trimmed_eof)
}

#[cfg(test)]
mod tests {
    use super::trim_start_whitespace;

    #[test]
    fn test_trim_start_whitespace() {
        assert_eq!(trim_start_whitespace("\r").0, "\r");
        assert_eq!(trim_start_whitespace("\rxxx").0, "\rxxx");
        assert_eq!(trim_start_whitespace("\t\rxxx").0, "\rxxx");
        assert_eq!(trim_start_whitespace("\r\n\rxxx").0, "\rxxx");

        assert_eq!(trim_start_whitespace("\n").0, "");
        assert_eq!(trim_start_whitespace("\r\n").0, "");
        assert_eq!(trim_start_whitespace("\t").0, "");
        assert_eq!(trim_start_whitespace(" ").0, "");

        assert_eq!(trim_start_whitespace("\nxxx").0, "xxx");
        assert_eq!(trim_start_whitespace("\r\nxxx").0, "xxx");
        assert_eq!(trim_start_whitespace("\txxx").0, "xxx");
        assert_eq!(trim_start_whitespace(" xxx").0, "xxx");

        assert_eq!(trim_start_whitespace(" \r\n").0, "");
        assert_eq!(trim_start_whitespace("\t\r\n").0, "");
        assert_eq!(trim_start_whitespace("\n\r\n").0, "");
        assert_eq!(trim_start_whitespace("\r\n ").0, "");
        assert_eq!(trim_start_whitespace("\r\n\t").0, "");
        assert_eq!(trim_start_whitespace("\r\n\n").0, "");

        assert_eq!(trim_start_whitespace(" \r\nxxx").0, "xxx");
        assert_eq!(trim_start_whitespace("\t\r\nxxx").0, "xxx");
        assert_eq!(trim_start_whitespace("\n\r\nxxx").0, "xxx");
        assert_eq!(trim_start_whitespace("\r\n xxx").0, "xxx");
        assert_eq!(trim_start_whitespace("\r\n\txxx").0, "xxx");
        assert_eq!(trim_start_whitespace("\r\n\nxxx").0, "xxx");

        assert_eq!(trim_start_whitespace(" \r\n\r\n").0, "");
        assert_eq!(trim_start_whitespace("\r\n \t\n").0, "");

        assert_eq!(trim_start_whitespace(" \r\n\r\nxxx").0, "xxx");
        assert_eq!(trim_start_whitespace("\r\n \t\nxxx").0, "xxx");

        assert_eq!(trim_start_whitespace(" \r\n\r\nxxx\n").0, "xxx\n");
        assert_eq!(trim_start_whitespace("\r\n \t\nxxx\r\n").0, "xxx\r\n");
        assert_eq!(trim_start_whitespace("\r\n\u{A0}\n").0, "\u{A0}\n");
        assert_eq!(trim_start_whitespace("\r\n\u{A0}\n").0, "\u{A0}\n");
        assert_eq!(trim_start_whitespace("\t  \u{0085}\n").0, "\u{0085}\n")
    }
}
