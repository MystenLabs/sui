// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    ptb::{
        ptb::PTBCommand,
        ptb_builder::{
            argument::Argument,
            argument_token::ArgumentToken,
            command_token::{CommandToken, ALL_PUBLIC_COMMAND_TOKENS},
            context::{FileScope, PTBContext},
            errors::{span, PTBError, Span, Spanned},
        },
    },
    sp,
};
use anyhow::{anyhow, bail, Context, Result as AResult};
use move_command_line_common::{
    address::{NumericalAddress, ParsedAddress},
    parser::{parse_u128, parse_u16, parse_u256, parse_u32, parse_u64, parse_u8, Parser, Token},
    types::TypeToken,
};
use move_core_types::identifier::Identifier;
use std::{error::Error, fmt::Debug, str::FromStr};

/// A parsed PTB command consisting of the command and the parsed arguments to the command.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ParsedPTBCommand {
    pub name: Spanned<CommandToken>,
    pub args: Vec<Spanned<Argument>>,
}

/// The parser for PTB command arguments.
pub struct ValueParser<'a> {
    inner: Spanner<'a>,
    current_scope: FileScope,
}

/// The PTB parsing context used when parsing PTB commands. This consists of:
/// - The list of alread-parsed commands
/// - The list of errors that have occured thus far during the parsing of the command(s)
///   - NB: errors are accumulated and returned at the end of parsing, instead of eagerly.
/// - The current file and command scope which is used for error reporting.
pub struct PTBParser {
    parsed: Vec<ParsedPTBCommand>,
    errors: Vec<PTBError>,
    context: PTBContext,
}

impl PTBParser {
    /// Create a new PTB parser.
    pub fn new() -> Self {
        Self {
            parsed: vec![],
            errors: vec![],
            context: PTBContext::new(),
        }
    }

    /// Return the list of parsed commands along with any errors that were encountered during the
    /// parsing of the PTB command(s).
    pub fn finish(self) -> (Vec<ParsedPTBCommand>, Vec<PTBError>) {
        (self.parsed, self.errors)
    }

    /// Parse a single PTB command. If an error is encountered, it is added to the list of
    /// `errors`.
    pub fn parse(&mut self, mut cmd: PTBCommand) {
        let name = CommandToken::from_str(&cmd.name);
        let name_span = Span::cmd_span(cmd.name.len(), self.context.current_file_scope());
        if let Err(e) = name {
            self.errors.push(PTBError::WithSource {
                message: format!("Failed to parse command name: {e}"),
                span: name_span,
                help: Some(format!(
                    "Valid commands are: {}",
                    ALL_PUBLIC_COMMAND_TOKENS.join(", ")
                )),
            });
            return;
        };
        let name = span(name_span, name.unwrap());

        match &name.value {
            CommandToken::FileEnd => {
                let fname = cmd.values.pop().unwrap();
                if let Err(e) = self.context.pop_file_scope(&fname) {
                    self.errors.push(e);
                }
                self.parsed.push(ParsedPTBCommand {
                    name,
                    args: vec![span(
                        Span::new(0, fname.len(), 0, self.context.current_file_scope()),
                        Argument::String(fname),
                    )],
                });
                return;
            }
            CommandToken::FileStart => {
                let fname = cmd.values.pop().unwrap();
                self.context.push_file_scope(fname.clone());
                self.parsed.push(ParsedPTBCommand {
                    name,
                    args: vec![span(
                        Span::new(0, fname.len(), 0, self.context.current_file_scope()),
                        Argument::String(fname),
                    )],
                });
                return;
            }
            CommandToken::Publish => {
                if cmd.values.len() != 1 {
                    self.errors.push(PTBError::WithSource {
                        message: format!(
                            "Invalid command -- expected 1 argument, got {}",
                            cmd.values.len()
                        ),
                        span: name_span,
                        help: None,
                    });
                    self.context.increment_file_command_index();
                    return;
                }
                self.parsed.push(ParsedPTBCommand {
                    name,
                    args: vec![Spanned {
                        span: Span {
                            start: 0,
                            end: cmd.values[0].len(),
                            arg_idx: 0,
                            file_scope: self.context.current_file_scope(),
                        },
                        value: Argument::String(cmd.values[0].clone()),
                    }],
                });
                self.context.increment_file_command_index();
                return;
            }
            CommandToken::Upgrade => {
                if cmd.values.len() != 2 {
                    self.errors.push(PTBError::WithSource {
                        message: format!(
                            "Invalid command -- expected 2 arguments, got {}",
                            cmd.values.len()
                        ),
                        span: name_span,
                        help: None,
                    });
                    self.context.increment_file_command_index();
                    return;
                }
                let mut upgrade_args = match Self::parse_values(
                    &cmd.values[0],
                    0,
                    self.context.current_file_scope(),
                ) {
                    Err(e) => {
                        self.errors.push(PTBError::WithSource {
                            message: format!("Failed to parse argument command. {}", e.message,),
                            span: e.span,
                            help: e.help,
                        });
                        self.context.increment_file_command_index();
                        return;
                    }
                    Ok(parsed) => parsed,
                };
                upgrade_args.push(Spanned {
                    span: Span {
                        start: 0,
                        end: cmd.values[1].len(),
                        arg_idx: 1,
                        file_scope: self.context.current_file_scope(),
                    },
                    value: Argument::String(cmd.values[1].clone()),
                });
                self.parsed.push(ParsedPTBCommand {
                    name,
                    args: upgrade_args,
                });
                self.context.increment_file_command_index();
                return;
            }
            _ => (),
        }
        let args = cmd
            .values
            .iter()
            .enumerate()
            .map(|(i, v)| Self::parse_values(&v, i, self.context.current_file_scope()))
            .collect::<ParsingResult<Vec<_>>>()
            .map_err(|e| PTBError::WithSource {
                message: format!(
                    "Failed to parse arguments for '{}' command. {}",
                    cmd.name, e.message
                ),
                span: e.span,
                help: e.help,
            });

        self.context.increment_file_command_index();

        match args {
            Ok(args) => {
                self.parsed.push(ParsedPTBCommand {
                    name,
                    args: args.into_iter().flatten().collect(),
                });
            }
            Err(e) => self.errors.push(e),
        }
    }

    /// Parse a string to a list of values. Values are separated by whitespace.
    fn parse_values(
        s: &str,
        arg_idx: usize,
        fscope: FileScope,
    ) -> ParsingResult<Vec<Spanned<Argument>>> {
        let tokens: Vec<_> = ArgumentToken::tokenize(s).map_err(|e| ParsingErr {
            span: Span::new(0, s.len(), arg_idx, fscope),
            message: e.into(),
            help: None,
        })?;
        let stokens = Spanner::new(tokens, arg_idx);
        let mut parser = ValueParser::new(stokens, fscope);
        let res = parser.parse_arguments()?;
        if let Ok(sp!(sp, (_, contents))) = parser.spanned(|p| p.advance_any()) {
            return Err(ParsingErr {
                span: sp,
                message: anyhow!("Expected end of token stream. Got: {}", contents).into(),
                help: None,
            });
        }
        Ok(res)
    }
}

/// A simple wrapper around a peekable-iterator-type interface that keeps track of the current
/// location in the input string that is being parsed. This is used to keep track of the location
/// for generation spans when parsing PTB arguments.
pub struct Spanner<'a> {
    current_location: usize,
    arg_idx: usize,
    tokens: Vec<(ArgumentToken, &'a str)>,
    /// Some arguments may permit non-whitespace delimitation after them (e.g., move-calls). So after
    /// parsng an arguments that permits no whitespace delimitation, we set this flag to true and
    /// the next token is guaranteed to be whitespace (inserted if not present).
    insert_non_whitespace_next_if_none: bool,
}

impl<'a> Spanner<'a> {
    fn new(mut tokens: Vec<(ArgumentToken, &'a str)>, arg_idx: usize) -> Self {
        tokens.reverse();
        Self {
            current_location: 0,
            arg_idx,
            tokens,
            insert_non_whitespace_next_if_none: false,
        }
    }

    fn insert_whitespace_next_if_none(&mut self) {
        self.insert_non_whitespace_next_if_none = true;
    }

    fn next(&mut self) -> Option<(ArgumentToken, &'a str)> {
        if self.insert_non_whitespace_next_if_none {
            self.insert_non_whitespace_next_if_none = false;
            if self
                .tokens
                .last()
                .map(|t| !t.0.is_whitespace())
                .unwrap_or(false)
            {
                return Some((ArgumentToken::Whitespace, " "));
            }
        }
        if let Some((tok, contents)) = self.tokens.pop() {
            self.current_location += contents.len();
            Some((tok, contents))
        } else {
            None
        }
    }

    fn peek(&self) -> Option<(ArgumentToken, &'a str)> {
        if self.insert_non_whitespace_next_if_none
            && self
                .tokens
                .last()
                .map(|t| !t.0.is_whitespace())
                .unwrap_or(false)
        {
            Some((ArgumentToken::Whitespace, " "))
        } else {
            self.tokens.last().copied()
        }
    }

    fn current_location(&self) -> usize {
        self.current_location
    }
}

impl<'a> Iterator for Spanner<'a> {
    type Item = (ArgumentToken, &'a str);

    fn next(&mut self) -> Option<Self::Item> {
        self.next()
    }
}

impl<'a> ValueParser<'a> {
    fn new(v: Spanner<'a>, current_scope: FileScope) -> Self {
        Self {
            inner: v,
            current_scope,
        }
    }

    fn advance_any(&mut self) -> AResult<(ArgumentToken, &'a str)> {
        match self.inner.next() {
            Some(tok) => Ok(tok),
            None => bail!("unexpected end of tokens"),
        }
    }

    fn advance(&mut self, expected_token: ArgumentToken) -> AResult<&'a str> {
        let (t, contents) = self.advance_any()?;
        if t != expected_token {
            bail!("expected token '{}', but got '{}'", expected_token, t)
        }
        Ok(contents)
    }

    fn peek_tok(&mut self) -> Option<ArgumentToken> {
        self.inner.peek().map(|(tok, _)| tok)
    }

    fn parse_list<R>(
        &mut self,
        parse_list_item: impl Fn(&mut Self) -> ParsingResult<R>,
        delim: ArgumentToken,
        end_token: ArgumentToken,
        allow_trailing_delim: bool,
    ) -> ParsingResult<Vec<R>> {
        let is_end = |tok_opt: Option<ArgumentToken>| -> bool {
            tok_opt.map(|tok| tok == end_token).unwrap_or(true)
        };
        let mut v = vec![];
        while !is_end(self.peek_tok()) {
            v.push(parse_list_item(self)?);
            if is_end(self.peek_tok()) {
                break;
            }
            self.spanned(|p| p.advance(delim))?;
            if is_end(self.peek_tok()) && allow_trailing_delim {
                break;
            }
        }
        Ok(v)
    }

    /// Parses a list of items separated by `delim` and terminated by `end_token`, skipping any
    /// tokens that match `skip`.
    /// This is used to parse lists of arguments, e.g. `1, 2, 3` or `1, 2, 3` where the tokenizer
    /// we're using is space-sensitive so we want to `skip` whitespace, and `delim` by ','.
    fn parse_list_skip<R>(
        &mut self,
        parse_list_item: impl Fn(&mut Self) -> ParsingResult<R>,
        delim: ArgumentToken,
        end_token: ArgumentToken,
        skip: ArgumentToken,
        allow_trailing_delim: bool,
    ) -> ParsingResult<Vec<R>> {
        let is_end = |parser: &mut Self| -> ParsingResult<bool> {
            while parser.peek_tok() == Some(skip) {
                parser.spanned(|p| p.advance(skip))?;
            }
            let is_end = parser
                .peek_tok()
                .map(|tok| tok == end_token)
                .unwrap_or(true);

            Ok(is_end)
        };
        let mut v = vec![];

        while !is_end(self)? {
            v.push(parse_list_item(self)?);
            if is_end(self)? {
                break;
            }
            self.spanned(|p| p.advance(delim))?;
            if is_end(self)? && allow_trailing_delim {
                break;
            }
        }
        Ok(v)
    }
}

pub struct ParsingErr {
    pub span: Span,
    pub message: Box<dyn Error>,
    pub help: Option<String>,
}

type ParsingResult<T> = Result<T, ParsingErr>;

impl<'a> ValueParser<'a> {
    /// Parse a list of arguments separated by whitespace.
    pub fn parse_arguments(&mut self) -> ParsingResult<Vec<Spanned<Argument>>> {
        let args = self.parse_list(
            |p| p.parse_argument(),
            ArgumentToken::Whitespace,
            /* not checked */ ArgumentToken::Void,
            /* allow_trailing_delim */ true,
        )?;
        Ok(args)
    }

    pub fn spanned<T: Debug + Clone + Eq + PartialEq, E: Into<Box<dyn Error>>>(
        &mut self,
        parse: impl Fn(&mut Self) -> Result<T, E>,
    ) -> ParsingResult<Spanned<T>> {
        let start = self.inner.current_location();
        let arg = parse(self);
        let end = self.inner.current_location();
        let sp = Span {
            start,
            end,
            arg_idx: self.inner.arg_idx,
            file_scope: self.current_scope,
        };
        let arg = arg.map_err(|e| ParsingErr {
            span: sp,
            message: e.into(),
            help: None,
        })?;
        Ok(span(sp, arg))
    }

    pub fn with_span<T: Debug + Clone + Eq + PartialEq, E: Into<Box<dyn Error>>>(
        &mut self,
        sp: Span,
        parse: impl Fn(&mut Self) -> Result<T, E>,
    ) -> ParsingResult<Spanned<T>> {
        let arg = parse(self);
        let arg = arg.map_err(|e| ParsingErr {
            span: sp,
            message: e.into(),
            help: None,
        })?;
        Ok(span(sp, arg))
    }

    pub fn sp<T: Debug + Clone + Eq + PartialEq>(&mut self, start_loc: usize, x: T) -> Spanned<T> {
        let end = self.inner.current_location();
        span(
            Span {
                start: start_loc,
                end,
                arg_idx: self.inner.arg_idx,
                file_scope: self.current_scope,
            },
            x,
        )
    }

    /// Parse a numerical address.
    fn parse_address(
        sp: Span,
        tok: ArgumentToken,
        contents: &str,
    ) -> ParsingResult<Spanned<ParsedAddress>> {
        let p_address = match tok {
            ArgumentToken::Ident => Ok(ParsedAddress::Named(contents.to_owned())),
            ArgumentToken::Number => NumericalAddress::parse_str(contents)
                .map_err(|s| anyhow!("Failed to parse address '{}'. Got error: {}", contents, s))
                .map(ParsedAddress::Numerical),
            _ => unreachable!(),
        };
        p_address
            .map(|addr| span(sp, addr))
            .map_err(|e| ParsingErr {
                span: sp,
                message: e.into(),
                help: None,
            })
    }

    /// Parse a single PTB argument. This is the main parsing function for PTB arguments.
    pub fn parse_argument(&mut self) -> ParsingResult<Spanned<Argument>> {
        use super::argument_token::ArgumentToken as Tok;
        use Argument as V;
        let begin_loc = self.inner.current_location();
        let sp!(tl_loc, arg) = self.spanned(|p| p.advance_any())?;
        Ok(match arg {
            (Tok::Ident, "true") => span(tl_loc, V::Bool(true)),
            (Tok::Ident, "false") => span(tl_loc, V::Bool(false)),
            (tok @ (Tok::Number | Tok::Ident), contents)
                if matches!(self.peek_tok(), Some(Tok::ColonColon)) =>
            {
                let address = Self::parse_address(tl_loc, tok, contents)?;
                self.spanned(|p| p.advance(Tok::ColonColon))?;
                let module_name = self.spanned(|parser| {
                    Identifier::new(
                        parser
                            .advance(Tok::Ident)
                            .with_context(|| format!("Unable to parse module name"))?,
                    )
                    .with_context(|| format!("Unable to parse module name"))
                    .into()
                })?;
                self.spanned(|p| {
                    p.advance(Tok::ColonColon)
                        .with_context(|| format!("Missing '::' after module name"))
                })?;
                let function_name = self.spanned(|p| {
                    Identifier::new(
                        p.advance(Tok::Ident)
                            .with_context(|| format!("Unable to parse function name"))?,
                    )
                })?;
                // Insert a whitepace before the type argument token if none is present.
                self.inner.insert_whitespace_next_if_none();
                self.sp(
                    begin_loc,
                    V::ModuleAccess {
                        address,
                        module_name,
                        function_name,
                    },
                )
            }
            (Tok::Number, contents) => {
                let num = self.with_span(tl_loc, |_| {
                    u64::from_str(contents).context("Invalid number")
                })?;
                span(num.span, V::U64(num.value))
            }
            (Tok::NumberTyped, contents) => {
                self.with_span::<Argument, anyhow::Error>(tl_loc, |_| {
                    Ok(if let Some(s) = contents.strip_suffix("u8") {
                        let (u, _) = parse_u8(s)?;
                        V::U8(u)
                    } else if let Some(s) = contents.strip_suffix("u16") {
                        let (u, _) = parse_u16(s)?;
                        V::U16(u)
                    } else if let Some(s) = contents.strip_suffix("u32") {
                        let (u, _) = parse_u32(s)?;
                        V::U32(u)
                    } else if let Some(s) = contents.strip_suffix("u64") {
                        let (u, _) = parse_u64(s)?;
                        V::U64(u)
                    } else if let Some(s) = contents.strip_suffix("u128") {
                        let (u, _) = parse_u128(s)?;
                        V::U128(u)
                    } else {
                        let (u, _) = parse_u256(contents.strip_suffix("u256").unwrap())?;
                        V::U256(u)
                    })
                })?
            }
            (Tok::At, _) => {
                let sp!(addr_span, (tok, contents)) = self.spanned(|p| p.advance_any())?;
                let sp = tl_loc.union_with([addr_span]);
                let address = Self::parse_address(sp, tok, contents)?;
                match address.value {
                    ParsedAddress::Named(n) => {
                        return self.with_span(sp, |_| bail!("Expected a numerical address at this position but got a named address {n}"));
                    }
                    ParsedAddress::Numerical(addr) => span(addr_span, V::Address(addr)),
                }
            }
            (Tok::Some_, _) => {
                self.spanned(|p| p.advance(Tok::LParen))?;
                let sp!(arg_span, arg) = self.parse_argument()?;
                let sp!(end_span, _) = self.spanned(|p| p.advance(Tok::RParen))?;
                let sp = tl_loc.union_with([arg_span, end_span]);
                span(sp, V::Option(span(arg_span, Some(Box::new(arg)))))
            }
            (Tok::None_, _) => span(tl_loc, V::Option(span(tl_loc, None))),
            (Tok::DoubleQuote, contents) => span(tl_loc, V::String(contents.to_owned())),
            (Tok::SingleQuote, contents) => span(tl_loc, V::String(contents.to_owned())),
            (Tok::Vector, _) => {
                self.spanned(|p| p.advance(Tok::LBracket))?;
                let values = self.parse_list_skip(
                    |p| p.parse_argument(),
                    ArgumentToken::Comma,
                    Tok::RBracket,
                    Tok::Whitespace,
                    /* allow_trailing_delim */ true,
                )?;
                let sp!(end_span, _) = self.spanned(|p| p.advance(Tok::RBracket))?;
                let total_span = tl_loc.union_with([end_span]);
                span(total_span, V::Vector(values))
            }
            (Tok::LBracket, _) => {
                let values = self.parse_list_skip(
                    |p| p.parse_argument(),
                    ArgumentToken::Comma,
                    Tok::RBracket,
                    Tok::Whitespace,
                    /* allow_trailing_delim */ true,
                )?;
                let sp!(end_span, _) = self.spanned(|p| p.advance(Tok::RBracket))?;
                let total_span = tl_loc.union_with([end_span]);
                span(total_span, V::Array(values))
            }
            (Tok::Ident, contents) if matches!(self.peek_tok(), Some(Tok::Dot)) => {
                // let sp!(_, prefix) = self.with_span(tl_loc, |_| contents)?;
                let mut fields = vec![];
                self.spanned(|p| p.advance(Tok::Dot))?;
                while let Ok(sp!(sp, (_, contents))) = self.spanned(|p| p.advance_any()) {
                    fields.push(span(sp, contents.to_string()));
                    if !matches!(self.peek_tok(), Some(Tok::Dot)) {
                        break;
                    }
                    self.spanned(|p| p.advance(Tok::Dot))?;
                }
                let sp = tl_loc.union_with(fields.iter().map(|f| f.span).collect::<Vec<_>>());
                span(
                    sp,
                    V::VariableAccess(span(tl_loc, contents.to_string()), fields),
                )
            }
            (Tok::Ident, contents) => span(tl_loc, V::Identifier(contents.to_string())),
            (Tok::TypeArgString, contents) => self.with_span(tl_loc, |_| {
                let type_tokens: Vec<_> = TypeToken::tokenize(contents)?
                    .into_iter()
                    .filter(|(tok, _)| !tok.is_whitespace())
                    .collect();
                let mut parser = Parser::new(type_tokens);
                parser.advance(TypeToken::Lt)?;
                let res =
                    parser.parse_list(|p| p.parse_type(), TypeToken::Comma, TypeToken::Gt, true)?;
                parser.advance(TypeToken::Gt)?;
                if let Ok((_, contents)) = parser.advance_any() {
                    bail!("Expected end of token stream. Got: {}", contents)
                }
                Ok(V::TyArgs(res))
            })?,
            (Tok::Gas, _) => span(tl_loc, V::Gas),
            x => self.with_span(tl_loc, |_| {
                bail!("unexpected token {:?}, expected argument", x)
            })?,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::ptb::ptb_builder::context::FileScope;
    use crate::ptb::ptb_builder::parse_ptb::PTBParser;

    fn dummy_file_scope() -> FileScope {
        FileScope {
            file_command_index: 0,
            name: "console".into(),
            name_index: 0,
        }
    }

    #[test]
    fn parse_value() {
        let values = vec![
            "true",
            "false",
            "1",
            "1u8",
            "1u16",
            "1u32",
            "1u64",
            "some(ident)",
            "some(123)",
            "some(@0x0)",
            "none",
        ];
        for s in &values {
            assert!(PTBParser::parse_values(s, 0, dummy_file_scope()).is_ok());
        }
    }

    #[test]
    fn parse_values() {
        let values = vec![
            "true @0x0 false 1 1u8",
            "true @0x0 false 1 1u8 vector_ident another ident",
            "true @0x0 false 1 1u8 some_ident another ident some(123) none",
            "true @0x0 false 1 1u8 some_ident another ident some(123) none vector[] [] [vector[]] [vector[1]] [vector[1,2]] [vector[1,2,]]",
        ];
        for s in &values {
            assert!(PTBParser::parse_values(s, 0, dummy_file_scope()).is_ok());
        }
    }

    #[test]
    fn parse_address() {
        let values = vec!["@0x0", "@1234", "foo"];
        for s in &values {
            assert!(PTBParser::parse_values(s, 0, dummy_file_scope()).is_ok());
        }
    }

    #[test]
    fn parse_string() {
        let values = vec!["\"hello world\"", "'hello world'"];
        for s in &values {
            assert!(PTBParser::parse_values(s, 0, dummy_file_scope()).is_ok());
        }
    }

    #[test]
    fn parse_vector() {
        let values = vec!["vector[]", "vector[1]", "vector[1,2]", "vector[1,2,]"];
        for s in &values {
            assert!(PTBParser::parse_values(s, 0, dummy_file_scope()).is_ok());
        }
    }

    #[test]
    fn parse_vector_with_spaces() {
        let values = vec!["vector[ ]", "vector[1 ]", "vector[1, 2]", "vector[1, 2, ]"];
        for s in &values {
            assert!(PTBParser::parse_values(s, 0, dummy_file_scope()).is_ok());
        }
    }

    #[test]
    fn parse_array() {
        let values = vec!["[]", "[1]", "[1,2]", "[1,2,]"];
        for s in &values {
            assert!(PTBParser::parse_values(s, 0, dummy_file_scope()).is_ok());
        }
    }

    #[test]
    fn parse_array_with_spaces() {
        let values = vec!["[ ]", "[1 ]", "[1, 2]", "[1, 2, ]"];
        for s in &values {
            assert!(PTBParser::parse_values(s, 0, dummy_file_scope()).is_ok());
        }
    }

    #[test]
    fn module_access() {
        let values = vec!["123::b::c", "0x0::b::c"];
        for s in &values {
            assert!(PTBParser::parse_values(s, 0, dummy_file_scope()).is_ok());
        }
    }

    #[test]
    fn type_args() {
        let values = vec!["<u64>", "<0x0::b::c>", "<0x0::b::c, 1234::g::f>"];
        for s in &values {
            assert!(PTBParser::parse_values(s, 0, dummy_file_scope()).is_ok());
        }
    }

    #[test]
    fn move_call() {
        let values = vec![
            "0x0::M::f",
            "<u64, 123::a::f<456::b::c>>",
            "1 2u32 vector[]",
        ];
        for s in &values {
            assert!(PTBParser::parse_values(s, 0, dummy_file_scope()).is_ok());
        }
    }

    #[test]
    fn dotted_accesses() {
        let values = vec!["a.0", "a.1.2", "a.0.1.2", "a.b.c", "a.b.c.d", "a.b.c.d.e"];
        for s in &values {
            assert!(PTBParser::parse_values(s, 0, dummy_file_scope()).is_ok());
        }
    }

    #[test]
    fn dotted_accesses_errs() {
        let values = vec!["a,1", "a.1,2"];
        for s in &values {
            assert!(PTBParser::parse_values(s, 0, dummy_file_scope()).is_err());
        }
    }
}
