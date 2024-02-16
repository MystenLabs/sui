// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    bind,
    client_ptb::{
        ptb::PTBCommand,
        ptb_builder::{
            argument::Argument,
            argument_token::ArgumentToken,
            command::GasPicker,
            command_token::{CommandToken, ALL_PUBLIC_COMMAND_TOKENS},
            context::{FileScope, PTBContext},
            errors::{span, PTBError, Span, Spanned},
        },
    },
    error, sp,
};
use anyhow::{anyhow, bail, Context, Result as AResult};
use move_command_line_common::{
    address::{NumericalAddress, ParsedAddress},
    parser::{parse_u128, parse_u16, parse_u256, parse_u32, parse_u64, parse_u8, Parser, Token},
    types::{ParsedType, TypeToken},
};
use move_core_types::identifier::Identifier;
use std::{error::Error, fmt::Debug, str::FromStr};

use super::{
    command::{ModuleAccess, ParsedPTBCommand},
    errors::PTBResult,
};

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
    errors: Vec<PTBError>,
    context: PTBContext,
    parsed: Vec<(Span, ParsedPTBCommand)>,
}

/// A simple wrapper around a peekable-iterator-type interface that keeps track of the current
/// location in the input string that is being parsed. This is used to keep track of the location
/// for generation spans when parsing PTB arguments.
pub struct Spanner<'a> {
    current_location: usize,
    arg_idx: usize,
    tokens: Vec<(ArgumentToken, &'a str)>,
}

pub struct ParsingErr {
    pub span: Span,
    pub message: Box<dyn Error>,
    pub help: Option<String>,
}

impl Default for PTBParser {
    fn default() -> Self {
        Self::new()
    }
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

    /// Return the list of parsed commands or any errors that were encountered during the
    /// parsing of the PTB command(s).
    pub fn finish(self) -> Result<Vec<(Span, ParsedPTBCommand)>, Vec<PTBError>> {
        println!("parsed: {:#?}", self.parsed);
        if self.errors.is_empty() {
            Ok(self.parsed)
        } else {
            Err(self.errors)
        }
    }

    /// Parse a single PTB command. If an error is encountered, it is added to the list of
    /// `errors`.
    pub fn parse_command(&mut self, cmd: PTBCommand) {
        println!("cmd: {:#?}", cmd);
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

        let values_string = cmd.values.join(" ");

        macro_rules! check_args {
            ($expected:expr, $cmd:expr) => {
                if !$expected.contains(&$cmd.values.len()) {
                    let formatted = if $expected.start() == $expected.end() {
                        format!("{}", $expected.start())
                    } else {
                        format!("{} to {}", $expected.start(), $expected.end())
                    };
                    self.errors.push(PTBError::WithSource {
                        message: format!(
                            "Invalid number of arguments: '{}' expected {} argument{} but got {}",
                            name.value,
                            formatted,
                            if $expected.start() == $expected.end() && *$expected.start() == 1 {
                                ""
                            } else {
                                "s"
                            },
                            $cmd.values.len()
                        ),
                        span: name_span,
                        help: None,
                    });
                    self.context.increment_file_command_index();
                    return;
                }
            };
        }

        macro_rules! handle_error {
            ($e:expr) => {
                if let Err(e) = $e {
                    self.errors.push(e);
                }
            };
        }

        match &name.value {
            CommandToken::FileStart => {
                handle_error!(self.parse_file_start(values_string));
                // Don't inrement the command index for file-start command since we've just entered
                // the file so early-return.
                return;
            }
            CommandToken::FileEnd => {
                check_args!(1..=1, cmd);
                handle_error!(self.parse_file_end(values_string));
                // Don't inrement the command index for file-end commands so early-return.
                return;
            }
            CommandToken::Publish => {
                check_args!(1..=1, cmd);
                handle_error!(self.parse_publish(name, values_string));
            }
            CommandToken::Upgrade => {
                check_args!(2..=2, cmd);
                handle_error!(self.parse_upgrade(name, values_string));
            }
            CommandToken::TransferObjects => {
                check_args!(2..=2, cmd);
                handle_error!(self.parse_transfer_objects(name, values_string));
            }
            CommandToken::SplitCoins => {
                check_args!(2..=2, cmd);
                handle_error!(self.parse_split_coins(name, cmd.values));
            }
            CommandToken::MergeCoins => {
                check_args!(2..=2, cmd);
                handle_error!(self.parse_merge_coins(name, cmd.values));
            }
            CommandToken::MakeMoveVec => {
                check_args!(2..=2, cmd);
                handle_error!(self.parse_make_move_vec(name, cmd.values));
            }
            CommandToken::MoveCall => {
                check_args!(1..=1024, cmd);
                handle_error!(self.parse_move_call(name, cmd.values));
            }
            CommandToken::Assign => {
                check_args!(1..=2, cmd);
                handle_error!(self.parse_assign(name, cmd.values));
            }
            CommandToken::WarnShadows => {
                check_args!(1..=1, cmd);
                handle_error!(self.parse_warn_shadows(name, cmd.values));
            }
            CommandToken::Preview => {
                check_args!(1..=1, cmd);
                handle_error!(self.parse_preview(name, cmd.values));
            }
            CommandToken::Summary => {
                check_args!(1..=1, cmd);
                handle_error!(self.parse_summary(name, cmd.values));
            }
            CommandToken::PickGasBudget => {
                check_args!(1..=1, cmd);
                handle_error!(self.parse_pick_gas_budget(name, cmd.values));
            }
            CommandToken::GasBudget => {
                check_args!(1..=1, cmd);
                handle_error!(self.parse_gas_budget(name, cmd.values));
            }
        };

        self.context.increment_file_command_index();
    }

    fn value_parser<'a>(&self, s: &'a [String], arg_idx: usize) -> PTBResult<ValueParser<'a>> {
        let fscope = self.context.current_file_scope();
        let s = s[arg_idx].as_str();
        let tokens: Vec<_> = ArgumentToken::tokenize(s).map_err(|e| PTBError::WithSource {
            span: Span::new(0, s.len(), arg_idx, fscope),
            message: e.to_string(),
            help: None,
        })?;
        let stokens = Spanner::new(tokens, arg_idx);
        Ok(ValueParser::new(stokens, fscope))
    }

    fn value_parser2<'a>(&self, s: &'a String) -> PTBResult<ValueParser<'a>> {
        let fscope = self.context.current_file_scope();
        let tokens: Vec<_> = ArgumentToken::tokenize(s).map_err(|e| PTBError::WithSource {
            // TODO: Can remove arg IDX from spans
            span: Span::new(0, s.len(), 0, fscope),
            message: e.to_string(),
            help: None,
        })?;
        let stokens = Spanner::new(tokens, 0);
        Ok(ValueParser::new(stokens, fscope))
    }

    fn parse_file_start(&mut self, value: String) -> PTBResult<()> {
        self.context.push_file_scope(value);
        Ok(())
    }

    fn parse_file_end(&mut self, value: String) -> PTBResult<()> {
        self.context.pop_file_scope(&value)
    }

    fn parse_publish(&mut self, name: Spanned<CommandToken>, value: String) -> PTBResult<()> {
        let sp = Span {
            start: 0,
            end: value.len(),
            arg_idx: 0,
            file_scope: self.context.current_file_scope(),
        };
        self.parsed.push((
            name.span,
            ParsedPTBCommand::Publish(Spanned {
                span: sp,
                value: value.clone(),
            }),
        ));
        Ok(())
    }

    fn parse_upgrade(&mut self, name: Spanned<CommandToken>, values: String) -> PTBResult<()> {
        let mut parser = self.value_parser2(&values)?;
        bind!(
            path_loc,
            Argument::String(s) | Argument::Identifier(s) = parser.parse_argument()?,
            |loc| { error!(loc, "Expected a string value for package path") }
        );
        parser.parse_whitespace()?;
        let cap_obj = parser.parse_argument()?;
        parser.end()?;
        self.parsed.push((
            name.span,
            ParsedPTBCommand::Upgrade(span(path_loc, s), cap_obj),
        ));
        Ok(())
    }

    fn parse_transfer_objects(
        &mut self,
        name: Spanned<CommandToken>,
        values: String,
    ) -> PTBResult<()> {
        let mut parser = self.value_parser2(&values)?;
        let transfer_to = parser.parse_argument()?;
        parser.parse_whitespace()?;
        let transfer_froms = parser.parse_array()?;
        parser.end()?;
        self.parsed.push((
            name.span,
            ParsedPTBCommand::TransferObjects(transfer_to, transfer_froms),
        ));
        Ok(())
    }

    fn parse_split_coins(
        &mut self,
        name: Spanned<CommandToken>,
        values: Vec<String>,
    ) -> PTBResult<()> {
        let split_from = self.value_parser(&values, 0)?.parse_single_argument()?;
        let splits = self.value_parser(&values, 1)?.parse_array()?;
        self.parsed
            .push((name.span, ParsedPTBCommand::SplitCoins(split_from, splits)));
        Ok(())
    }

    fn parse_merge_coins(
        &mut self,
        name: Spanned<CommandToken>,
        values: Vec<String>,
    ) -> PTBResult<()> {
        let merge_into = self.value_parser(&values, 0)?.parse_single_argument()?;
        let coins = self.value_parser(&values, 1)?.parse_array()?;
        self.parsed
            .push((name.span, ParsedPTBCommand::MergeCoins(merge_into, coins)));
        Ok(())
    }

    fn parse_make_move_vec(
        &mut self,
        name: Spanned<CommandToken>,
        values: Vec<String>,
    ) -> PTBResult<()> {
        let sp!(loc, mut tys) = self.value_parser(&values, 0)?.parse_type_args()?;
        if tys.len() != 1 {
            error!(loc, "Expected a single type argument",)
        }
        let ty = tys.pop().unwrap();
        let array = self.value_parser(&values, 1)?.parse_array()?;
        self.parsed.push((
            name.span,
            ParsedPTBCommand::MakeMoveVec(span(loc, ty.clone()), array),
        ));
        Ok(())
    }

    fn parse_move_call(
        &mut self,
        name: Spanned<CommandToken>,
        values: Vec<String>,
    ) -> PTBResult<()> {
        let (module_access, mut tys_opt) = self.value_parser(&values, 0)?.parse_module_access()?;

        let mut args = None;

        for i in 1..values.len() {
            let mut parser = self.value_parser(&values, i)?;
            if let Some(ArgumentToken::TypeArgString) = parser.peek_tok() {
                let tys = parser.parse_type_args()?;
                if let Some(other_tys) = tys_opt {
                    error!(
                        tys.span,
                        "Type arguments already specified in function call but also supplied here"
                    )
                }
                tys_opt = Some(tys);
            } else {
                let inner_args = args.get_or_insert_with(Vec::new);
                inner_args.append(&mut parser.parse_list(
                    |p| p.parse_argument(),
                    ArgumentToken::Whitespace,
                    /* not checked */ ArgumentToken::Void,
                    /* allow_trailing_delim */ true,
                )?);
            }
        }

        self.parsed.push((
            name.span,
            ParsedPTBCommand::MoveCall(module_access, tys_opt, args.unwrap_or_else(Vec::new)),
        ));
        Ok(())
    }

    fn parse_assign(&mut self, name: Spanned<CommandToken>, values: Vec<String>) -> PTBResult<()> {
        bind!(
            assign_loc,
            Argument::Identifier(s) = self.value_parser(&values, 0)?.parse_single_argument()?,
            |loc| { error!(loc, "Expected variable binding") }
        );
        let assign_to = if values.len() == 2 {
            let assign_to = self.value_parser(&values, 1)?.parse_single_argument()?;
            Some(assign_to)
        } else {
            None
        };
        self.parsed.push((
            name.span,
            ParsedPTBCommand::Assign(span(assign_loc, s), assign_to),
        ));
        Ok(())
    }

    fn parse_warn_shadows(
        &mut self,
        name: Spanned<CommandToken>,
        values: Vec<String>,
    ) -> PTBResult<()> {
        bind!(
            loc,
            Argument::Bool(b) = self.value_parser(&values, 0)?.parse_single_argument()?,
            |loc| { error!(loc, "Expected a boolean value") }
        );
        self.parsed.push((
            name.span,
            ParsedPTBCommand::WarnShadows(span(loc, Argument::Bool(b))),
        ));
        Ok(())
    }

    fn parse_preview(&mut self, name: Spanned<CommandToken>, values: Vec<String>) -> PTBResult<()> {
        bind!(
            loc,
            Argument::Bool(b) = self.value_parser(&values, 0)?.parse_single_argument()?,
            |loc| { error!(loc, "Expected a boolean value") }
        );
        self.parsed.push((
            name.span,
            ParsedPTBCommand::Preview(span(loc, Argument::Bool(b))),
        ));
        Ok(())
    }
    fn parse_summary(
        &mut self,
        _name: Spanned<CommandToken>,
        values: Vec<String>,
    ) -> PTBResult<()> {
        bind!(
            _loc,
            Argument::Bool(_) = self.value_parser(&values, 0)?.parse_single_argument()?,
            |loc| { error!(loc, "Expected a boolean value") }
        );
        Ok(())
    }
    fn parse_pick_gas_budget(
        &mut self,
        name: Spanned<CommandToken>,
        values: Vec<String>,
    ) -> PTBResult<()> {
        bind!(
            loc,
            Argument::Identifier(s) = self.value_parser(&values, 0)?.parse_single_argument()?,
            |loc| { error!(loc, "Expected a string value") }
        );
        let picker = match s.as_str() {
            "max" => GasPicker::Max,
            "sum" => GasPicker::Sum,
            x => error!(loc, "Invalid gas picker: {}", x),
        };
        self.parsed.push((
            name.span,
            ParsedPTBCommand::PickGasBudget(span(loc, picker)),
        ));
        Ok(())
    }

    fn parse_gas_budget(
        &mut self,
        name: Spanned<CommandToken>,
        values: Vec<String>,
    ) -> PTBResult<()> {
        bind!(
            loc,
            Argument::U64(u) = self.value_parser(&values, 0)?.parse_single_argument()?,
            |loc| { error!(loc, "Expected a u64 value") }
        );
        self.parsed
            .push((name.span, ParsedPTBCommand::GasBudget(span(loc, u))));
        Ok(())
    }
}

impl<'a> Spanner<'a> {
    fn new(mut tokens: Vec<(ArgumentToken, &'a str)>, arg_idx: usize) -> Self {
        tokens.reverse();
        Self {
            current_location: 0,
            arg_idx,
            tokens,
        }
    }

    fn next(&mut self) -> Option<(ArgumentToken, &'a str)> {
        if let Some((tok, contents)) = self.tokens.pop() {
            self.current_location += contents.len();
            Some((tok, contents))
        } else {
            None
        }
    }

    fn peek(&self) -> Option<(ArgumentToken, &'a str)> {
        self.tokens.last().copied()
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
        parse_list_item: impl Fn(&mut Self) -> PTBResult<R>,
        delim: ArgumentToken,
        end_token: ArgumentToken,
        allow_trailing_delim: bool,
    ) -> PTBResult<Vec<R>> {
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
        parse_list_item: impl Fn(&mut Self) -> PTBResult<R>,
        delim: ArgumentToken,
        end_token: ArgumentToken,
        skip: ArgumentToken,
        allow_trailing_delim: bool,
    ) -> PTBResult<Vec<R>> {
        let is_end = |parser: &mut Self| -> PTBResult<bool> {
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

impl<'a> ValueParser<'a> {
    fn spanned<T: Debug + Clone + Eq + PartialEq, E: Into<Box<dyn Error>>>(
        &mut self,
        parse: impl Fn(&mut Self) -> Result<T, E>,
    ) -> PTBResult<Spanned<T>> {
        let start = self.inner.current_location();
        let arg = parse(self);
        let end = self.inner.current_location();
        let sp = Span {
            start,
            end,
            arg_idx: self.inner.arg_idx,
            file_scope: self.current_scope,
        };
        let arg = arg.map_err(|e| PTBError::WithSource {
            span: sp,
            message: e.into().to_string(),
            help: None,
        })?;
        Ok(span(sp, arg))
    }

    pub fn with_span<T: Debug + Clone + Eq + PartialEq, E: Into<Box<dyn Error>>>(
        &mut self,
        sp: Span,
        parse: impl Fn(&mut Self) -> Result<T, E>,
    ) -> PTBResult<Spanned<T>> {
        let arg = parse(self);
        let arg = arg.map_err(|e| PTBError::WithSource {
            span: sp,
            message: e.into().to_string(),
            help: None,
        })?;
        Ok(span(sp, arg))
    }

    fn sp<T: Debug + Clone + Eq + PartialEq>(&mut self, start_loc: usize, x: T) -> Spanned<T> {
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

    // Parse a single argument, and make sure that there are no trailing tokens (other than
    // possibly whitespace) after the argument.
    fn parse_single_argument(&mut self) -> PTBResult<Spanned<Argument>> {
        let arg = self.parse_argument()?;

        // Skip any whitespace after the argument
        while self.peek_tok() == Some(ArgumentToken::Whitespace) {
            self.spanned(|p| p.advance(ArgumentToken::Whitespace))?;
        }

        // Check if there are trailing tokens after the argument
        if let Ok(sp!(s, (tok, _))) = self.spanned(|p| p.advance_any()) {
            error!(s, "Unexpected token '{}'", tok);
        }

        Ok(arg)
    }

    fn end(mut self) -> PTBResult<()> {
        while self.peek_tok() == Some(ArgumentToken::Whitespace) {
            self.spanned(|p| p.advance(ArgumentToken::Whitespace))?;
        }
        if let Ok(sp!(s, (tok, _))) = self.spanned(|p| p.advance_any()) {
            error!(s, "Unexpected token '{}'", tok);
        }
        Ok(())
    }

    // Parse a single PTB argument and allow trailing characters possibly.
    fn parse_argument(&mut self) -> PTBResult<Spanned<Argument>> {
        use super::argument_token::ArgumentToken as Tok;
        use Argument as V;
        let sp!(tl_loc, arg) = self.spanned(|p| p.advance_any())?;
        Ok(match arg {
            (Tok::Ident, "true") => span(tl_loc, V::Bool(true)),
            (Tok::Ident, "false") => span(tl_loc, V::Bool(false)),
            (Tok::Number, contents) => {
                let num =
                    self.with_span::<u64, anyhow::Error>(tl_loc, |_| Ok(parse_u64(contents)?.0))?;
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
            (Tok::Vector, _) => self.parse_array()?.map(V::Vector),
            (Tok::Ident, contents) if matches!(self.peek_tok(), Some(Tok::Dot)) => {
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
            (Tok::Gas, _) => span(tl_loc, V::Gas),
            x => self.with_span(tl_loc, |_| {
                bail!("unexpected token '{}'", x.1);
            })?,
        })
    }

    /// Parse a numerical or named address.
    fn parse_address(
        sp: Span,
        tok: ArgumentToken,
        contents: &str,
    ) -> PTBResult<Spanned<ParsedAddress>> {
        let p_address = match tok {
            ArgumentToken::Ident => Ok(ParsedAddress::Named(contents.to_owned())),
            ArgumentToken::Number => NumericalAddress::parse_str(contents)
                .map_err(|s| anyhow!("Failed to parse address '{}'. Got error: {}", contents, s))
                .map(ParsedAddress::Numerical),
            _ => error!(sp => help: {
                    "Valid addresses can either be a variable in-scope, or a numerical address, e.g., 0xc0ffee"
                 },
                 "Expected an address"
            ),
        };
        p_address
            .map(|addr| span(sp, addr))
            .map_err(|e| PTBError::WithSource {
                span: sp,
                message: e.to_string(),
                help: None,
            })
    }

    // Parse a list of type arguments
    fn parse_type_args(&mut self) -> PTBResult<Spanned<Vec<ParsedType>>> {
        let sp!(tl_loc, contents) = self.spanned(|p| p.advance(ArgumentToken::TypeArgString))?;
        self.with_span(tl_loc, |_| {
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
            Ok(res)
        })
    }

    // Parse an array of arguments. These each element of the array is separated by a comma and the
    // parsing is not whitespace-sensitive.
    fn parse_array(&mut self) -> PTBResult<Spanned<Vec<Spanned<Argument>>>> {
        let sp!(start_loc, _) = self.spanned(|p| p.advance(ArgumentToken::LBracket))?;
        let values = self.parse_list_skip(
            |p| p.parse_argument(),
            ArgumentToken::Comma,
            ArgumentToken::RBracket,
            ArgumentToken::Whitespace,
            /* allow_trailing_delim */ true,
        )?;
        let sp!(end_span, _) = self.spanned(|p| p.advance(ArgumentToken::RBracket))?;
        let total_span = start_loc.union_with([end_span]);

        Ok(span(total_span, values))
    }

    // Parse a module access, which consists of an address, module name, and function name. If
    // type arguments are also present, they are parsed and returned as well otherwise `None` is
    // returned for them.
    fn parse_module_access(
        &mut self,
    ) -> PTBResult<(Spanned<ModuleAccess>, Option<Spanned<Vec<ParsedType>>>)> {
        let begin_loc = self.inner.current_location();
        let sp!(tl_loc, (tok, contents)) = self.spanned(|p| p.advance_any())?;
        let address = Self::parse_address(tl_loc, tok, contents)?;
        self.spanned(|p| p.advance(ArgumentToken::ColonColon))?;
        let module_name = self.spanned(|parser| {
            Identifier::new(
                parser
                    .advance(ArgumentToken::Ident)
                    .with_context(|| "Unable to parse module name".to_string())?,
            )
            .with_context(|| "Unable to parse module name".to_string())
        })?;
        self.spanned(|p| {
            p.advance(ArgumentToken::ColonColon)
                .with_context(|| "Missing '::' after module name".to_string())
        })?;
        let function_name = self.spanned(|p| {
            Identifier::new(
                p.advance(ArgumentToken::Ident)
                    .with_context(|| "Unable to parse function name".to_string())?,
            )
        })?;
        let module_access = self.sp(
            begin_loc,
            ModuleAccess {
                address,
                module_name,
                function_name,
            },
        );

        while self.peek_tok() == Some(ArgumentToken::Whitespace) {
            self.spanned(|p| p.advance(ArgumentToken::Whitespace))?;
        }

        let ty_args_opt = if let Some(ArgumentToken::TypeArgString) = self.peek_tok() {
            Some(self.parse_type_args()?)
        } else {
            None
        };
        Ok((module_access, ty_args_opt))
    }

    // Consume at least one whitespace token, and then consume any additional whitespace tokens.
    fn parse_whitespace(&mut self) -> PTBResult<()> {
        self.spanned(|p| p.advance(ArgumentToken::Whitespace))?;
        while self.peek_tok() == Some(ArgumentToken::Whitespace) {
            self.spanned(|p| p.advance(ArgumentToken::Whitespace))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::client_ptb::ptb_builder::{argument_token::ArgumentToken, parse_ptb::PTBParser};

    #[test]
    fn parse_value() {
        let values = vec![
            "true".to_string(),
            "false".to_string(),
            "1".to_string(),
            "1u8".to_string(),
            "1u16".to_string(),
            "1u32".to_string(),
            "1u64".to_string(),
            "some(ident)".to_string(),
            "some(123)".to_string(),
            "some(@0x0)".to_string(),
            "none".to_string(),
        ];
        let parser = PTBParser::new();
        for i in 0..values.len() {
            assert!(parser
                .value_parser(&values, i)
                .unwrap()
                .parse_single_argument()
                .is_ok());
        }
    }

    #[test]
    fn parse_values() {
        let values = vec![
            "".to_string(),
            "true".to_string(),
            "true @0x0 false 1 1u8".to_string(),
            "true @0x0 false 1 1u8 vector_ident another ident".to_string(),
            "true @0x0 false 1 1u8 some_ident another ident some(123) none".to_string(),
            "true @0x0 false 1 1u8 some_ident another ident some(123) none vector[] vector[] vector[1] vector[1,vector[2]] vector[1,2,]".to_string(),
        ];
        let parser = PTBParser::new();
        for i in 0..values.len() {
            assert!(parser
                .value_parser(&values, i)
                .unwrap()
                .parse_list(
                    |p| p.parse_argument(),
                    ArgumentToken::Whitespace,
                    /* not checked */ ArgumentToken::Void,
                    /* allow_trailing_delim */ true,
                )
                .is_ok());
        }
    }

    #[test]
    fn parse_address() {
        let values = vec!["@0x0".to_string(), "@1234".to_string(), "foo".to_string()];
        let parser = PTBParser::new();
        for i in 0..values.len() {
            assert!(parser
                .value_parser(&values, i)
                .unwrap()
                .parse_argument()
                .is_ok());
        }
    }

    #[test]
    fn parse_string() {
        let values = vec!["\"hello world\"".to_string(), "'hello world'".to_string()];
        let parser = PTBParser::new();
        for i in 0..values.len() {
            assert!(parser
                .value_parser(&values, i)
                .unwrap()
                .parse_single_argument()
                .is_ok());
        }
    }

    #[test]
    fn parse_vector() {
        let values = vec![
            "vector[]".to_string(),
            "vector[1]".to_string(),
            "vector[1,2]".to_string(),
            "vector[1,2,]".to_string(),
        ];
        let parser = PTBParser::new();
        for i in 0..values.len() {
            assert!(parser
                .value_parser(&values, i)
                .unwrap()
                .parse_single_argument()
                .is_ok());
        }
    }

    #[test]
    fn parse_vector_with_spaces() {
        let values = vec![
            "vector[ ]".to_string(),
            "vector[1 ]".to_string(),
            "vector[1, 2]".to_string(),
            "vector[1, 2, ]".to_string(),
        ];
        let parser = PTBParser::new();
        for i in 0..values.len() {
            assert!(parser
                .value_parser(&values, i)
                .unwrap()
                .parse_single_argument()
                .is_ok());
        }
    }

    #[test]
    fn parse_array() {
        let values = vec![
            "[]".to_string(),
            "[1]".to_string(),
            "[1,2]".to_string(),
            "[1,2,]".to_string(),
        ];
        let parser = PTBParser::new();
        for i in 0..values.len() {
            assert!(parser
                .value_parser(&values, i)
                .unwrap()
                .parse_array()
                .is_ok());
        }
    }

    #[test]
    fn parse_array_with_spaces() {
        let values = vec![
            "[ ]".to_string(),
            "[1 ]".to_string(),
            "[1, 2]".to_string(),
            "[1, 2, ]".to_string(),
        ];
        let parser = PTBParser::new();
        for i in 0..values.len() {
            assert!(parser
                .value_parser(&values, i)
                .unwrap()
                .parse_array()
                .is_ok());
        }
    }

    #[test]
    fn module_access() {
        let values = vec![
            "123::b::c".to_string(),
            "0x0::b::c".to_string(),
            "std::bar::foo<u64>".to_string(),
            "std::bar::baz    <u64, bool>".to_string(),
        ];
        let parser = PTBParser::new();
        for i in 0..values.len() {
            assert!(parser
                .value_parser(&values, i)
                .unwrap()
                .parse_module_access()
                .is_ok());
        }
    }

    #[test]
    fn type_args() {
        let values = vec![
            "<u64>".to_string(),
            "<0x0::b::c>".to_string(),
            "<0x0::b::c, 1234::g::f>".to_string(),
        ];
        let parser = PTBParser::new();
        for i in 0..values.len() {
            assert!(parser
                .value_parser(&values, i)
                .unwrap()
                .parse_type_args()
                .is_ok());
        }
    }

    #[test]
    fn dotted_accesses() {
        let values = vec![
            "a.0".to_string(),
            "a.1.2".to_string(),
            "a.0.1.2".to_string(),
            "a.b.c".to_string(),
            "a.b.c.d".to_string(),
            "a.b.c.d.e".to_string(),
        ];
        let parser = PTBParser::new();
        for i in 0..values.len() {
            assert!(parser
                .value_parser(&values, i)
                .unwrap()
                .parse_single_argument()
                .is_ok());
        }
    }

    #[test]
    fn dotted_accesses_errs() {
        let values = vec!["a,1".to_string(), "a.1,2".to_string()];
        let parser = PTBParser::new();
        for i in 0..values.len() {
            assert!(parser
                .value_parser(&values, i)
                .unwrap()
                .parse_single_argument()
                .is_err());
        }
    }
}
