// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    bind,
    client_ptb::ptb_builder::{
        argument::Argument,
        context::FileScope,
        errors::{span, PTBError, Span, Spanned},
        token::PTBToken,
    },
    error, sp,
};
use anyhow::{anyhow, bail, Context, Result as AResult};
use linked_hash_map::LinkedHashMap;
use move_command_line_common::{
    address::{NumericalAddress, ParsedAddress},
    parser::{parse_u128, parse_u16, parse_u256, parse_u32, parse_u64, parse_u8, Parser, Token},
    types::{ParsedType, TypeToken},
};
use move_core_types::identifier::Identifier;
use move_symbol_pool::Symbol;
use std::{collections::BTreeMap, error::Error, fmt::Debug};
use sui_types::base_types::ObjectID;

use super::{
    ast::{GasPicker, ModuleAccess, ParsedPTBCommand, ParsedProgram, Program, ProgramMetadata},
    errors::PTBResult, utils::read_ptb_file,
};

/// Parse a program
pub struct ProgramParser<'a> {
    file_table: &'a mut BTreeMap<Symbol, String>,
    current_scope: Scope<'a>,
    file_scopes: LinkedHashMap<Symbol, Scope<'a>>,
    seen_scopes: BTreeMap<Symbol, usize>,
    state: ProgramParsingState,
}

struct ProgramParsingState {
    parsed: Vec<Spanned<ParsedPTBCommand>>,
    errors: Vec<PTBError>,
    preview_set: bool,
    summary_set: bool,
    warn_shadows_set: bool,
    json_set: bool,
    gas_object_id: Option<Spanned<ObjectID>>,
}

/// A `Scope` is a single file scope that we are parsing. It holds the current tokens, the current
/// scope for error reporting, and the current parsed commands and errors in that file or "scope".
pub struct Scope<'a> {
    pub tokens: Spanner<'a>,
    pub current_scope: FileScope,
}

/// A `Spanner` is a simple wrapper around a vector of tokens that allows us to iterate over them.
/// It keeps track of our current location in the token stream (useful for creating spans) as well
/// as consuming any whitespace tokens.
pub struct Spanner<'a> {
    pub current_location: usize,
    tokens: Vec<(PTBToken, &'a str)>,
}

/// Messages that can be returned from parsing a scope.
pub enum ScopeParsingResult {
    // Hit a file include, so need the top-level parser to swap the current scope with the new one.
    File(Spanned<String>),
    // Done parsing this scope.
    Done,
}

impl<'a> ProgramParser<'a> {
    pub fn new(
        starting_contents: String,
        file_table: &'a mut BTreeMap<Symbol, String>,
    ) -> PTBResult<Self> {
        let name = Symbol::from("console");
        file_table.insert(name, starting_contents);
        let current_scope = Scope::new(name, file_table[&name].clone(), 0)?;
        Ok(Self {
            file_table,
            current_scope,
            file_scopes: LinkedHashMap::new(),
            seen_scopes: [(name, 0)].into_iter().collect(),
            state: ProgramParsingState {
                parsed: vec![],
                errors: vec![],
                preview_set: false,
                summary_set: false,
                warn_shadows_set: false,
                json_set: false,
                gas_object_id: None,
            },
        })
    }

    pub fn parse(mut self) -> Result<ParsedProgram, Vec<PTBError>> {
        loop {
            // If current scope is done, finish it and pop up to the previous scope
            // If there are no more scopes, we are done so break the loop.
            if self.current_scope.is_done() {
                if let Some((_, scope)) = self.file_scopes.pop_back() {
                    self.current_scope = scope;
                } else {
                    break;
                }
            }

            // Parse current scope
            let result = match self.current_scope.parse(&mut self.state) {
                Ok(r) => r,
                Err(e) => {
                    self.state.errors.push(e);
                    continue;
                }
            };

            match result {
                // If done, we will handle popping/swapping scopes on the next iteration
                ScopeParsingResult::Done => continue,
                // If we hit a file include command, we will swap the current scope with the new one
                ScopeParsingResult::File(sp!(loc, name)) => {
                    let file_path = self.current_scope.current_scope.qualify_path(&name);
                    let Ok(file_contents) = read_ptb_file(&file_path) else {
                        self.state.errors.push(PTBError::WithSource {
                            span: loc,
                            message: format!("Unable to read file '{:?}'", file_path),
                            help: None,
                        });
                        continue;
                    };
                    let name = Symbol::from(file_path.to_str().unwrap());
                    let name_index = self
                        .seen_scopes
                        .entry(name)
                        .and_modify(|i| *i += 1)
                        .or_insert(0);
                    self.file_table.insert(name, file_contents);
                    let Ok(new_scope) =
                        Scope::new(name, self.file_table[&name].clone(), *name_index)
                    else {
                        self.state.errors.push(PTBError::WithSource {
                            span: loc,
                            message: format!("Unable to parse file '{:?}'", file_path),
                            help: None,
                        });
                        continue;
                    };
                    if self
                        .file_scopes
                        .insert(name, std::mem::replace(&mut self.current_scope, new_scope))
                        .is_some()
                    {
                        self.state.errors.push(PTBError::WithSource {
                            span: loc,
                            message: format!("Cyclic file dependency found with '{}'", name),
                            help: None,
                        });
                        break;
                    }
                }
            }
        }

        if self.state.errors.is_empty() {
            Ok((
                Program {
                    commands: self.state.parsed,
                    warn_shadows_set: self.state.warn_shadows_set,
                },
                ProgramMetadata {
                    preview_set: self.state.preview_set,
                    summary_set: self.state.summary_set,
                    gas_object_id: self.state.gas_object_id,
                    json_set: self.state.json_set,
                },
            ))
        } else {
            Err(self.state.errors)
        }
    }
}

impl<'a> Scope<'a> {
    fn new(name: Symbol, contents: String, name_index: usize) -> PTBResult<Self> {
        let fscope = FileScope {
            file_command_index: 0,
            name,
            name_index,
        };
        let len = contents.len();
        // TODO: handle this leak
        let tokens: Vec<_> = PTBToken::tokenize(Box::leak(Box::new(contents))).map_err(|e| {
            PTBError::WithSource {
                span: Span::new(0, len, fscope),
                message: e.to_string(),
                help: None,
            }
        })?;
        Ok(Self {
            tokens: Spanner::new(tokens),
            current_scope: fscope,
        })
    }

    fn fast_forward_to_command(&mut self) {
        while self
            .tokens
            .peek()
            .is_some_and(|(tok, _)| !tok.is_command_token())
        {
            self.tokens.next();
        }
    }

    fn parse(&mut self, parsing_state: &mut ProgramParsingState) -> PTBResult<ScopeParsingResult> {
        // Consume whitespace and comments before getting the starting location so we don't include
        // them in the span.
        let mut starting_loc = self.tokens.current_location_before();

        while let Some((tok, _c)) = self.tokens.next() {
            match tok {
                c @ (PTBToken::CommandTransferObjects
                | PTBToken::CommandSplitCoins
                | PTBToken::CommandMergeCoins
                | PTBToken::CommandMakeMoveVec
                | PTBToken::CommandMoveCall
                | PTBToken::CommandPublish
                | PTBToken::CommandUpgrade
                | PTBToken::CommandAssign
                | PTBToken::CommandPickGasBudget
                | PTBToken::CommandGasBudget) => match self.parse_ptb_command(c) {
                    Ok(cmd) => parsing_state.parsed.push(Spanned {
                        span: Span::new(
                            starting_loc,
                            self.tokens.current_location(),
                            self.current_scope,
                        ),
                        value: cmd,
                    }),
                    Err(e) => {
                        parsing_state.errors.push(e);
                        // Try to find a new command token, or EOF so we can keep going
                        self.fast_forward_to_command();
                    }
                },
                PTBToken::CommandWarnShadows => parsing_state.warn_shadows_set = true,
                PTBToken::CommandPreview => parsing_state.preview_set = true,
                PTBToken::CommandSummary => parsing_state.summary_set = true,
                PTBToken::CommandJson => parsing_state.json_set = true,
                PTBToken::CommandGas => match self.parse_gas_specifier() {
                    Ok(gas) => parsing_state.gas_object_id = Some(gas),
                    Err(e) => {
                        parsing_state.errors.push(e);
                        self.fast_forward_to_command();
                    }
                },
                PTBToken::CommandFile => match self.parse_file_name() {
                    Ok(name) => {
                        self.current_scope.increment_file_command_index();
                        return Ok(ScopeParsingResult::File(name));
                    }
                    Err(e) => parsing_state.errors.push(e),
                },

                PTBToken::Comment => (),

                _ => error!(
                    Span::new(
                        starting_loc,
                        self.tokens.current_location(),
                        self.current_scope
                    ),
                    "Unexpected token '{}'", tok
                ),
            }
            self.current_scope.increment_file_command_index();
            starting_loc = self.tokens.current_location_before();
        }
        Ok(ScopeParsingResult::Done)
    }

    fn is_done(&self) -> bool {
        self.tokens.peek().is_none()
    }
}

impl<'a> Spanner<'a> {
    fn new(mut tokens: Vec<(PTBToken, &'a str)>) -> Self {
        tokens.reverse();
        Self {
            current_location: 0,
            tokens,
        }
    }

    fn next(&mut self) -> Option<(PTBToken, &'a str)> {
        self.consume_whitepace();

        if let Some((tok, contents)) = self.tokens.pop() {
            self.current_location += contents.len();
            Some((tok, contents))
        } else {
            None
        }
    }

    fn peek(&self) -> Option<(PTBToken, &'a str)> {
        self.tokens
            .iter()
            .rposition(|(tok, _)| !tok.is_whitespace() && tok != &PTBToken::Comment)
            .and_then(|i| self.tokens.get(i).copied())
    }

    fn current_location(&self) -> usize {
        self.current_location
    }

    fn current_location_before(&mut self) -> usize {
        self.consume_whitepace();
        self.current_location
    }

    fn consume_whitepace(&mut self) {
        while let Some((tok, contents)) = self.tokens.last() {
            if tok.is_whitespace() || tok == &PTBToken::Comment {
                self.current_location += contents.len();
                self.tokens.pop();
            } else {
                break;
            }
        }
    }
}

// Parser impls for a single command
impl<'a> Scope<'a> {
    fn parse_ptb_command(&mut self, command: PTBToken) -> PTBResult<ParsedPTBCommand> {
        match command {
            PTBToken::CommandPublish => self.parse_publish(),
            PTBToken::CommandUpgrade => self.parse_upgrade(),
            PTBToken::CommandTransferObjects => self.parse_transfer_objects(),
            PTBToken::CommandSplitCoins => self.parse_split_coins(),
            PTBToken::CommandMergeCoins => self.parse_merge_coins(),
            PTBToken::CommandMakeMoveVec => self.parse_make_move_vec(),
            PTBToken::CommandMoveCall => self.parse_move_call(),
            PTBToken::CommandAssign => self.parse_assign(),
            PTBToken::CommandPickGasBudget => self.parse_pick_gas_budget(),
            PTBToken::CommandGasBudget => self.parse_gas_budget(),
            PTBToken::CommandWarnShadows
            | PTBToken::CommandPreview
            | PTBToken::CommandSummary
            | PTBToken::CommandJson
            | PTBToken::CommandGas => {
                unreachable!()
            }
            _ => unreachable!(),
        }
    }

    fn parse_file_name(&mut self) -> PTBResult<Spanned<String>> {
        let sp!(path_loc, arg) = self.parse_argument()?;
        match arg {
            Argument::String(s) => Ok(span(path_loc, s)),
            Argument::Identifier(s) => Ok(span(path_loc, s)),
            Argument::VariableAccess(s, rest) => Ok(span(
                path_loc,
                format!(
                    "{}.{}",
                    s.value,
                    rest.into_iter()
                        .map(|f| f.value)
                        .collect::<Vec<_>>()
                        .join(".")
                ),
            )),
            _ => error!(path_loc, "Expected a string value for path"),
        }
    }

    fn parse_publish(&mut self) -> PTBResult<ParsedPTBCommand> {
        Ok(ParsedPTBCommand::Publish(self.parse_file_name()?))
    }

    fn parse_upgrade(&mut self) -> PTBResult<ParsedPTBCommand> {
        let s = self.parse_file_name()?;
        let cap_obj = self.parse_argument()?;
        Ok(ParsedPTBCommand::Upgrade(s, cap_obj))
    }

    fn parse_transfer_objects(&mut self) -> PTBResult<ParsedPTBCommand> {
        let transfer_to = self.parse_argument()?;
        let transfer_froms = self.parse_array()?;
        Ok(ParsedPTBCommand::TransferObjects(
            transfer_to,
            transfer_froms,
        ))
    }

    fn parse_split_coins(&mut self) -> PTBResult<ParsedPTBCommand> {
        let split_from = self.parse_argument()?;
        let splits = self.parse_array()?;
        Ok(ParsedPTBCommand::SplitCoins(split_from, splits))
    }

    fn parse_merge_coins(&mut self) -> PTBResult<ParsedPTBCommand> {
        let merge_into = self.parse_argument()?;
        let coins = self.parse_array()?;
        Ok(ParsedPTBCommand::MergeCoins(merge_into, coins))
    }

    fn parse_make_move_vec(&mut self) -> PTBResult<ParsedPTBCommand> {
        let sp!(loc, mut tys) = self.parse_type_args()?;
        if tys.len() != 1 {
            error!(loc, "Expected a single type argument",)
        }
        let ty = tys.pop().unwrap();
        let array = self.parse_array()?;
        Ok(ParsedPTBCommand::MakeMoveVec(span(loc, ty.clone()), array))
    }

    fn parse_move_call(&mut self) -> PTBResult<ParsedPTBCommand> {
        let (module_access, mut tys_opt) = self.parse_module_access()?;

        let mut args = None;

        while let Some(tok) = self.tokens.peek() {
            if tok.0.is_command_token() {
                break;
            }
            if PTBToken::TypeArgString == tok.0 {
                let tys = self.parse_type_args()?;
                if tys_opt.is_some() {
                    error!(
                        tys.span,
                        "Type arguments already specified in function call but also supplied here"
                    )
                }
                tys_opt = Some(tys);
            } else {
                let inner_args = args.get_or_insert_with(Vec::new);
                inner_args.push(self.parse_argument()?);
            }
        }

        Ok(ParsedPTBCommand::MoveCall(
            module_access,
            tys_opt,
            args.unwrap_or_else(Vec::new),
        ))
    }

    fn parse_assign(&mut self) -> PTBResult<ParsedPTBCommand> {
        bind!(
            assign_loc,
            Argument::Identifier(s) = self.parse_argument()?,
            |loc| { error!(loc, "Expected variable binding") }
        );

        let assign_to = if !matches!(self.tokens.peek(), Some(tok) if tok.0.is_command_token()) {
            Some(self.parse_argument()?)
        } else {
            None
        };

        Ok(ParsedPTBCommand::Assign(span(assign_loc, s), assign_to))
    }

    fn parse_pick_gas_budget(&mut self) -> PTBResult<ParsedPTBCommand> {
        bind!(
            loc,
            Argument::Identifier(s) = self.parse_argument()?,
            |loc| { error!(loc, "Expected a string value") }
        );
        let picker = match s.as_str() {
            "max" => GasPicker::Max,
            "sum" => GasPicker::Sum,
            x => error!(loc, "Invalid gas picker: {}", x),
        };
        Ok(ParsedPTBCommand::PickGasBudget(span(loc, picker)))
    }

    fn parse_gas_budget(&mut self) -> PTBResult<ParsedPTBCommand> {
        bind!(loc, Argument::U64(u) = self.parse_argument()?, |loc| {
            error!(loc, "Expected a u64 value")
        });
        Ok(ParsedPTBCommand::GasBudget(span(loc, u)))
    }

    fn parse_gas_specifier(&mut self) -> PTBResult<Spanned<ObjectID>> {
        bind!(loc, Argument::Address(a) = self.parse_argument()?, |loc| {
            error!(loc, "Expected an address")
        });
        Ok(span(loc, ObjectID::from(a.into_inner())))
    }
}

// Core token stream consumption, parsing, and span generation helpers
impl<'a> Scope<'a> {
    fn advance_any(&mut self) -> AResult<(PTBToken, &'a str)> {
        match self.tokens.next() {
            Some(tok) => Ok(tok),
            None => bail!("unexpected end of tokens"),
        }
    }

    fn advance(&mut self, expected_token: PTBToken) -> AResult<&'a str> {
        let (t, contents) = self.advance_any()?;
        if t != expected_token {
            bail!("expected token '{}', but got '{}'", expected_token, t)
        }
        Ok(contents)
    }

    fn peek_tok(&mut self) -> Option<PTBToken> {
        self.tokens.peek().map(|(tok, _)| tok)
    }

    fn parse_list<R>(
        &mut self,
        parse_list_item: impl Fn(&mut Self) -> PTBResult<R>,
        delim: PTBToken,
        end_token: PTBToken,
        allow_trailing_delim: bool,
    ) -> PTBResult<Vec<R>> {
        let is_end = |tok_opt: Option<PTBToken>| -> bool {
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

    fn spanned<T: Debug + Clone + Eq + PartialEq, E: Into<Box<dyn Error>>>(
        &mut self,
        parse: impl Fn(&mut Self) -> Result<T, E>,
    ) -> PTBResult<Spanned<T>> {
        let start = self.tokens.current_location_before();
        let arg = parse(self);
        let end = self.tokens.current_location();
        let sp = Span {
            start,
            end,
            file_scope: self.current_scope,
        };
        let arg = arg.map_err(|e| PTBError::WithSource {
            span: sp,
            message: e.into().to_string(),
            help: None,
        })?;
        Ok(span(sp, arg))
    }

    fn with_span<T: Debug + Clone + Eq + PartialEq, E: Into<Box<dyn Error>>>(
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
}

// Core argument parsing
impl<'a> Scope<'a> {
    // Parse a single PTB argument and allow trailing characters possibly.
    fn parse_argument(&mut self) -> PTBResult<Spanned<Argument>> {
        use super::token::PTBToken as Tok;
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
            (Tok::DoubleQuote, contents) => span(
                tl_loc,
                V::String(contents[1..contents.len() - 1].to_owned()),
            ),
            (Tok::SingleQuote, contents) => span(
                tl_loc,
                V::String(contents[1..contents.len() - 1].to_owned()),
            ),
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
    fn parse_address(sp: Span, tok: PTBToken, contents: &str) -> PTBResult<Spanned<ParsedAddress>> {
        let p_address = match tok {
            PTBToken::Ident => Ok(ParsedAddress::Named(contents.to_owned())),
            PTBToken::Number => NumericalAddress::parse_str(contents)
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
        self.tokens.consume_whitepace();
        let sp!(tl_loc, contents) = self.spanned(|p| p.advance(PTBToken::TypeArgString))?;
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
        self.tokens.consume_whitepace();
        let sp!(start_loc, _) = self.spanned(|p| p.advance(PTBToken::LBracket))?;
        let values = self.parse_list(
            |p| p.parse_argument(),
            PTBToken::Comma,
            PTBToken::RBracket,
            /* allow_trailing_delim */ true,
        )?;
        let sp!(end_span, _) = self.spanned(|p| p.advance(PTBToken::RBracket))?;
        let total_span = start_loc.union_with([end_span]);

        Ok(span(total_span, values))
    }

    // Parse a module access, which consists of an address, module name, and function name. If
    // type arguments are also present, they are parsed and returned as well otherwise `None` is
    // returned for them.
    fn parse_module_access(
        &mut self,
    ) -> PTBResult<(Spanned<ModuleAccess>, Option<Spanned<Vec<ParsedType>>>)> {
        let begin_loc = self.tokens.current_location_before();
        let sp!(tl_loc, (tok, contents)) = self.spanned(|p| p.advance_any())?;
        let address = Self::parse_address(tl_loc, tok, contents)?;
        self.spanned(|p| p.advance(PTBToken::ColonColon))?;
        let module_name = self.spanned(|parser| {
            Identifier::new(
                parser
                    .advance(PTBToken::Ident)
                    .with_context(|| "Unable to parse module name".to_string())?,
            )
            .with_context(|| "Unable to parse module name".to_string())
        })?;
        self.spanned(|p| {
            p.advance(PTBToken::ColonColon)
                .with_context(|| "Missing '::' after module name".to_string())
        })?;
        let function_name = self.spanned(|p| {
            Identifier::new(
                p.advance(PTBToken::Ident)
                    .with_context(|| "Unable to parse function name".to_string())?,
            )
        })?;
        let module_access = span(
            Span::new(
                begin_loc,
                self.tokens.current_location(),
                self.current_scope,
            ),
            ModuleAccess {
                address,
                module_name,
                function_name,
            },
        );

        while self.peek_tok() == Some(PTBToken::Whitespace) {
            self.spanned(|p| p.advance(PTBToken::Whitespace))?;
        }

        let ty_args_opt = if let Some(PTBToken::TypeArgString) = self.peek_tok() {
            Some(self.parse_type_args()?)
        } else {
            None
        };
        Ok((module_access, ty_args_opt))
    }
}
