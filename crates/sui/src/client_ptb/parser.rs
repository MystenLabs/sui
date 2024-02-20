// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::iter::Peekable;

use move_command_line_common::{
    address::{NumericalAddress, ParsedAddress},
    parser::{parse_u128, parse_u16, parse_u256, parse_u32, parse_u64, parse_u8},
};
use sui_types::base_types::ObjectID;

use crate::{bind_, client_ptb::ast::GasPicker, err_, error_, sp_};

use super::{
    ast::{self as A, Argument, ParsedPTBCommand, ParsedProgram},
    error::{PTBError, PTBResult, Span, Spanned},
    lexer::Lexer,
    token::{Lexeme, Token},
};

/// Parse a program
pub struct ProgramParser<'a, I: Iterator<Item = &'a str>> {
    tokens: Peekable<Lexer<'a, I>>,
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

impl<'a, I: Iterator<Item = &'a str>> ProgramParser<'a, I> {
    /// Create a PTB program parser from a sequence of string.
    pub fn new(tokens: I) -> PTBResult<Self> {
        let Some(tokens) = Lexer::new(tokens) else {
            error_!(Span { start: 0, end: 0 }, "No tokens")
        };
        Ok(Self {
            tokens: tokens.peekable(),
            state: ProgramParsingState {
                parsed: Vec::new(),
                errors: Vec::new(),
                preview_set: false,
                summary_set: false,
                warn_shadows_set: false,
                json_set: false,
                gas_object_id: None,
            },
        })
    }

    /// Parse the sequence of strings into a PTB program. We continue to parse even if an error is
    /// raised, and return the errors at the end. If no errors are raised, we return the parsed PTB
    /// program along with the metadata that we have parsed (e.g., json output, summary).
    pub fn parse(mut self) -> Result<ParsedProgram, Vec<PTBError>> {
        macro_rules! handle {
            ($expr: expr) => {
                match $expr {
                    Ok(arg) => arg,
                    Err(err) => {
                        self.state.errors.push(err);
                        self.fast_forward_to_next_command();
                        continue;
                    }
                }
            };
        }
        while let Some(sp_!(lspan, lexeme)) = self.tokens.next() {
            match lexeme.token {
                Token::Command => {
                    let parsed = match lexeme.src {
                        A::TRANSFER_OBJECTS => self.parse_transfer_objects(lspan),
                        A::SPLIT_COINS => self.parse_split_coins(lspan),
                        A::MERGE_COINS => self.parse_merge_coins(lspan),
                        A::ASSIGN => self.parse_assign(lspan),
                        A::GAS_BUDGET => self.parse_gas_budget(lspan),
                        A::PICK_GAS_BUDGET => self.parse_pick_gas_budget(lspan),
                        A::GAS_COIN => {
                            let specifier = self.parse_gas_specifier(lspan);
                            self.state.gas_object_id = Some(handle!(specifier));
                            continue;
                        }
                        A::SUMMARY => {
                            self.state.summary_set = true;
                            continue;
                        }
                        A::JSON => {
                            self.state.json_set = true;
                            continue;
                        }
                        A::PREVIEW => {
                            self.state.preview_set = true;
                            continue;
                        }
                        A::WARN_SHADOWS => {
                            self.state.warn_shadows_set = true;
                            continue;
                        }

                        A::MAKE_MOVE_VEC => todo!(),
                        A::MOVE_CALL => todo!(),

                        A::PUBLISH => unreachable!(),
                        A::UPGRADE => unreachable!(),
                        _ => {
                            self.state.errors.push(err_!(
                                lspan,
                                "Unknown command: '{}'",
                                lexeme.src
                            ));
                            self.fast_forward_to_next_command();
                            continue;
                        }
                    };
                    let spanned_command = handle!(parsed);
                    self.state.parsed.push(spanned_command);
                }
                Token::Publish => {
                    let spanned_command =
                        lspan.wrap(ParsedPTBCommand::Publish(lspan.wrap(lexeme.src.to_owned())));
                    self.state.parsed.push(spanned_command);
                }
                Token::Upgrade => {
                    let arg = handle!(self.parse_argument(lspan));
                    let spanned_command = lspan.wrap(ParsedPTBCommand::Upgrade(
                        lspan.wrap(lexeme.src.to_owned()),
                        arg,
                    ));
                    self.state.parsed.push(spanned_command);
                }

                Token::Ident
                | Token::Number
                | Token::HexNumber
                | Token::String
                | Token::ColonColon
                | Token::Comma
                | Token::LBracket
                | Token::RBracket
                | Token::LParen
                | Token::RParen
                | Token::LAngle
                | Token::RAngle
                | Token::At
                | Token::Dot => {
                    self.state.errors.push(PTBError {
                        message: format!("Unexpected token: {:?}", lexeme.token),
                        span: lspan,
                        help: Some("Expected to find a command here".to_string()),
                    });
                    self.fast_forward_to_next_command();
                }
                Token::Unexpected => {
                    self.state
                        .errors
                        .push(err_!(lspan, "Unexpected token '{}'", lexeme.src));
                    break;
                }
                Token::UnfinishedString => todo!(),
                Token::EarlyEof => {
                    self.state
                        .errors
                        .push(err_!(lspan, "Unexpected end of tokens"));
                    break;
                }
            };
        }

        if self.state.errors.is_empty() {
            Ok((
                A::Program {
                    commands: self.state.parsed,
                    warn_shadows_set: self.state.warn_shadows_set,
                },
                A::ProgramMetadata {
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

/// Iterator convenience methods over tokens
impl<'a, I: Iterator<Item = &'a str>> ProgramParser<'a, I> {
    /// Advance the iterator and return the next token. If the next token is not the expected one,
    /// return an error.
    fn advance(&mut self, current_loc: Span, expected: Token) -> PTBResult<Spanned<Lexeme<'a>>> {
        let sp_!(sp, lxm) = self.advance_any(current_loc)?;
        if lxm.token != expected {
            error_!(
                sp,
                "Expected token '{:?}' but found '{:?}'",
                expected,
                lxm.token
            )
        }
        Ok(sp.wrap(lxm))
    }

    /// Advance the iterator and return the next token. If there is no next token, return an error.
    fn advance_any(&mut self, current_loc: Span) -> PTBResult<Spanned<Lexeme<'a>>> {
        let Some(sp_!(sp, lxm)) = self.tokens.next() else {
            error_!(current_loc, "Unexpected end of tokens")
        };
        Ok(sp.wrap(lxm))
    }

    /// Peek at the next token without advancing the iterator.
    fn peek_tok(&mut self) -> Option<Token> {
        self.tokens.peek().map(|sp_!(_, lxm)| lxm.token)
    }

    /// Fast forward to the next command token (if any).
    fn fast_forward_to_next_command(&mut self) {
        while let Some(tok) = self.peek_tok() {
            if tok == Token::Command {
                break;
            }
            self.tokens.next();
        }
    }
}

/// Methods for parsing commands
impl<'a, I: Iterator<Item = &'a str>> ProgramParser<'a, I> {
    /// Parse a transfer-objects command.
    /// The expected format is: `--transfer-objects <to> [<from>, ...]`
    fn parse_transfer_objects(&mut self, loc_ctx: Span) -> PTBResult<Spanned<ParsedPTBCommand>> {
        let transfer_to = self.parse_argument(loc_ctx)?;
        let transfer_froms = self.parse_array(transfer_to.span)?;
        Ok(loc_ctx
            .widen(transfer_froms.span)
            .wrap(ParsedPTBCommand::TransferObjects(
                transfer_to,
                transfer_froms,
            )))
    }

    /// Parse a split-coins command.
    /// The expected format is: `--split-coins <coin> [<amount>, ...]`
    fn parse_split_coins(&mut self, loc_ctx: Span) -> PTBResult<Spanned<ParsedPTBCommand>> {
        let split_from = self.parse_argument(loc_ctx)?;
        let splits = self.parse_array(split_from.span)?;
        Ok(loc_ctx
            .widen(splits.span)
            .wrap(ParsedPTBCommand::SplitCoins(split_from, splits)))
    }

    /// Parse a merge-coins command.
    /// The expected format is: `--merge-coins <coin> [<coin1>, ...]`
    fn parse_merge_coins(&mut self, loc_ctx: Span) -> PTBResult<Spanned<ParsedPTBCommand>> {
        let merge_into = self.parse_argument(loc_ctx)?;
        let coins = self.parse_array(merge_into.span)?;
        Ok(loc_ctx
            .widen(coins.span)
            .wrap(ParsedPTBCommand::MergeCoins(merge_into, coins)))
    }

    /// Parse an assign command.
    /// The expected format is: `--assign <variable> (<value>)?`
    fn parse_assign(&mut self, loc_ctx: Span) -> PTBResult<Spanned<ParsedPTBCommand>> {
        bind_!(
            assign_loc,
            Argument::Identifier(s) = self.parse_argument(loc_ctx)?,
            |loc| { error_!(loc, "Expected variable binding") }
        );

        let assign_to = if self.peek_tok().is_some_and(|tok| tok != Token::Command) {
            Some(self.parse_argument(assign_loc)?)
        } else {
            None
        };

        Ok(loc_ctx
            .widen(assign_to.as_ref().map(|a| a.span).unwrap_or(assign_loc))
            .wrap(ParsedPTBCommand::Assign(assign_loc.wrap(s), assign_to)))
    }

    /// Parse a pick-gas-budget command.
    /// The expected format is: `--pick-gas-budget <picker>` where picker is either `max` or `sum`.
    fn parse_pick_gas_budget(&mut self, loc_ctx: Span) -> PTBResult<Spanned<ParsedPTBCommand>> {
        bind_!(
            loc,
            Argument::Identifier(s) = self.parse_argument(loc_ctx)?,
            |loc| { error_!(loc, "Expected a string value") }
        );
        let picker = match s.as_str() {
            "max" => GasPicker::Max,
            "sum" => GasPicker::Sum,
            x => error_!(loc, "Invalid gas picker: {}", x),
        };
        Ok(loc_ctx
            .widen(loc)
            .wrap(ParsedPTBCommand::PickGasBudget(loc.wrap(picker))))
    }

    /// Parse a gas-budget command.
    /// The expected format is: `--gas-budget <u64>`
    fn parse_gas_budget(&mut self, loc_ctx: Span) -> PTBResult<Spanned<ParsedPTBCommand>> {
        bind_!(
            loc,
            Argument::U64(u) = self.parse_argument(loc_ctx)?,
            |loc| { error_!(loc, "Expected a u64 value") }
        );
        Ok(loc_ctx
            .widen(loc)
            .wrap(ParsedPTBCommand::GasBudget(loc.wrap(u))))
    }

    /// Parse a gas specifier.
    /// The expected format is: `--gas-coin <address>`
    fn parse_gas_specifier(&mut self, loc_ctx: Span) -> PTBResult<Spanned<ObjectID>> {
        bind_!(
            loc,
            Argument::Address(a) = self.parse_argument(loc_ctx)?,
            |loc| { error_!(loc, "Expected an address") }
        );
        Ok(loc.wrap(ObjectID::from(a.into_inner())))
    }
}

/// Methods for parsing arguments and types in commands
impl<'a, I: Iterator<Item = &'a str>> ProgramParser<'a, I> {
    // Parse a single PTB argument and allow trailing characters possibly.
    fn parse_argument(&mut self, current_span: Span) -> PTBResult<Spanned<Argument>> {
        use super::token::Token as Tok;
        use Argument as V;
        let sp_!(tl_loc, arg) = self.advance_any(current_span)?;
        Ok(match (arg.token, arg.src) {
            (Tok::Ident, "true") => tl_loc.wrap(V::Bool(true)),
            (Tok::Ident, "false") => tl_loc.wrap(V::Bool(false)),
            (Tok::HexNumber | Tok::Number, num_contents) => {
                let num_contents = if arg.token == Tok::HexNumber {
                    format!("0x{}", num_contents)
                } else {
                    num_contents.to_owned()
                };
                macro_rules! parse_num {
                    ($fn: ident, $ty: expr, $sp: expr) => {{
                        self.tokens.next().unwrap();
                        match $fn(&num_contents) {
                            Ok((value, _)) => tl_loc.widen($sp).wrap($ty(value)),
                            Err(e) => error_!(tl_loc.widen($sp), "{}", e),
                        }
                    }};
                }
                match self
                    .tokens
                    .peek()
                    .map(|lxm| (lxm.span, lxm.value.token, lxm.value.src))
                {
                    Some((sp, Tok::Ident, A::U8)) => {
                        parse_num!(parse_u8, V::U8, sp)
                    }
                    Some((sp, Tok::Ident, A::U16)) => {
                        parse_num!(parse_u16, V::U16, sp)
                    }
                    Some((sp, Tok::Ident, A::U32)) => {
                        parse_num!(parse_u32, V::U32, sp)
                    }
                    Some((sp, Tok::Ident, A::U64)) => {
                        parse_num!(parse_u64, V::U64, sp)
                    }
                    Some((sp, Tok::Ident, A::U128)) => {
                        parse_num!(parse_u128, V::U128, sp)
                    }
                    Some((sp, Tok::Ident, A::U256)) => {
                        parse_num!(parse_u256, V::U256, sp)
                    }
                    Some(_) | None => parse_u64(&num_contents)
                        .map(|(value, _)| tl_loc.wrap(V::U64(value)))
                        .map_err(|e| err_!(tl_loc, "{}", e))?,
                }
            }
            (Tok::At, _) => {
                let Some(sp_!(addr_span, lxm)) = self.tokens.next() else {
                    error_!(
                        tl_loc,
                        "Unexpected end of tokens: Expected an address literal."
                    )
                };
                let sp = tl_loc.widen(addr_span);
                let address = Self::parse_address(sp, lxm)?;
                match address.value {
                    ParsedAddress::Named(n) => {
                        error_!(sp, "Expected a numerical address at this position but got a named address '{n}'")
                    }
                    ParsedAddress::Numerical(addr) => sp.wrap(V::Address(addr)),
                }
            }
            (Tok::Ident, A::NONE) => tl_loc.wrap(V::Option(tl_loc.wrap(None))),
            (Tok::Ident, A::SOME) => {
                let sp_!(aloc, _) = self.advance(tl_loc, Tok::LParen)?;
                let sp_!(arg_span, arg) = self.parse_argument(aloc)?;
                let sp_!(end_span, _) = self.advance(arg_span, Tok::RParen)?;
                let sp = tl_loc.widen(end_span);
                sp.wrap(V::Option(arg_span.wrap(Some(Box::new(arg)))))
            }
            (Tok::String, contents) => tl_loc.wrap(V::String(contents.to_owned())),
            (Tok::Ident, A::VECTOR) => self.parse_array(tl_loc)?.map(V::Vector),

            (Tok::Ident, contents) if self.peek_tok() == Some(Tok::Dot) => {
                let mut fields = vec![];
                let sp_!(mut l, _) = self.advance(tl_loc, Tok::Dot)?;
                if self.peek_tok().is_none() {
                    error_!(l, "Expected a field name after '.'")
                }
                while let Ok(sp_!(sp, lxm)) = self.advance_any(l) {
                    fields.push(sp.wrap(lxm.src.to_string()));
                    if self.peek_tok() != Some(Tok::Dot) {
                        break;
                    }
                    let sp_!(loc, _) = self.advance(sp, Tok::Dot)?;
                    l = loc;
                }
                let sp = fields
                    .last()
                    .map(|f| f.span.widen(tl_loc))
                    .unwrap_or(tl_loc);
                sp.wrap(V::VariableAccess(tl_loc.wrap(contents.to_string()), fields))
            }
            (Tok::Ident, A::GAS) => tl_loc.wrap(V::Gas),
            (Tok::Ident, contents) => tl_loc.wrap(V::Identifier(contents.to_string())),
            (_, src) => error_!(tl_loc, "Unexpected token: '{}'", src),
        })
    }

    /// Parse a numerical or named address.
    fn parse_address(sp: Span, lxm: Lexeme<'a>) -> PTBResult<Spanned<ParsedAddress>> {
        use super::token::Token as Tok;
        let p_address = match lxm.token {
            Tok::Ident => Ok(ParsedAddress::Named(lxm.src.to_owned())),
            Tok::Number | Tok::HexNumber => NumericalAddress::parse_str(lxm.src)
                .map_err(|s| err_!(sp, "Failed to parse address '{}' {}", lxm.src, s))
                .map(ParsedAddress::Numerical),
            _ => error_!(sp => help: {
                    "Valid addresses can either be a variable in-scope, or a numerical address, e.g., 0xc0ffee"
                 },
                 "Expected an address"
            ),
        };
        p_address.map(|addr| sp.wrap(addr)).map_err(|e| PTBError {
            span: sp,
            message: e.to_string(),
            help: None,
        })
    }

    // Parse an array of arguments. Each element of the array is separated by a comma.
    fn parse_array(&mut self, mut current_loc: Span) -> PTBResult<Spanned<Vec<Spanned<Argument>>>> {
        use super::token::Token as Tok;
        let sp_!(start_loc, _) = self.advance(current_loc, Tok::LBracket)?;

        let mut values = vec![];
        if self.peek_tok().is_none() {
            error_!(start_loc, "Unexpected end of tokens: Expected an array")
        }
        while self.peek_tok() != Some(Tok::RBracket) {
            let sp_!(sp, arg) = self.parse_argument(current_loc)?;
            values.push(sp.wrap(arg));
            current_loc = sp;
            if self.peek_tok() == Some(Tok::RBracket) {
                break;
            }
            let sp_!(l, _) = self.advance(current_loc, Tok::Comma)?;
            current_loc = l;
            if self.peek_tok() == Some(Tok::RBracket) {
                break;
            }
        }
        let sp_!(end_span, _) = self.advance(current_loc, Tok::RBracket)?;
        let total_span = start_loc.widen(end_span);

        Ok(total_span.wrap(values))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse() {
        let input = "--transfer-objects a [b, c]";
        let x = shlex::split(input).unwrap();
        let parser = ProgramParser::new(x.iter().map(|x| x.as_str())).unwrap();
        let result = parser.parse();
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_unexpected_top_level() {
        let input = "\"0x\" ";
        let x = shlex::split(input).unwrap();
        let parser = ProgramParser::new(x.iter().map(|x| x.as_str())).unwrap();
        let result = parser.parse();
        insta::assert_debug_snapshot!(result.unwrap_err());
    }

    #[test]
    fn test_parse_publish() {
        let inputs = vec!["--publish \"foo/bar\"", "--publish foo/bar"];
        let mut parsed = Vec::new();
        for input in inputs {
            let x = shlex::split(input).unwrap();
            let parser = ProgramParser::new(x.iter().map(|x| x.as_str())).unwrap();
            let result = parser.parse().unwrap();
            parsed.push(result);
        }
        insta::assert_debug_snapshot!(parsed);
    }

    #[test]
    fn test_parse_args() {
        let inputs = vec![
            // Bools
            "true",
            "false",
            // Integers
            "1",
            "1_000",
            "100_000_000",
            "100_000u64",
            "1u8",
            "1_u128",
            "0x1",
            "0x1_000",
            "0x100_000_000",
            "0x100_000u64",
            "0x1u8",
            "0x1_u128",
            // Addresses
            "@0x1",
            "@0x1_000",
            "@0x100_000_000",
            "@0x100_000u64",
            "@0x1u8",
            "@0x1_u128",
            // Option
            "none",
            "some(1)",
            "some(0x1)",
            "some(some(some(some(1u128))))",
            // vector
            "vector[]",
            "vector[1, 2, 3]",
            "vector[1, 2, 3,]",
            // Dotted access
            "foo.bar",
            "foo.bar.baz",
            "foo.bar.baz.qux",
            "foo.0",
            "foo.0.1",
        ];
        let mut parsed = Vec::new();
        for input in inputs {
            let x = shlex::split(input).unwrap();
            let mut parser = ProgramParser::new(x.iter().map(|x| x.as_str())).unwrap();
            let result = parser
                .parse_argument(Span {
                    start: 0,
                    end: input.len(),
                })
                .unwrap();
            parsed.push(result);
        }
        insta::assert_debug_snapshot!(parsed);
    }

    #[test]
    fn test_parse_args_invalid() {
        let inputs = vec![
            // Integers
            "0xfffu8", // addresses
            "@n",      // options
            "some",
            "some(",
            "some(1",
            // vectors
            "vector",
            "vector[",
            "vector[1,",
            "vector[1 2]",
            "vector[,]",
            // Dotted access
            "foo.",
            ".",
        ];
        let mut parsed = Vec::new();
        for input in inputs {
            let x = shlex::split(input).unwrap();
            let mut parser = ProgramParser::new(x.iter().map(|x| x.as_str())).unwrap();
            let result = parser
                .parse_argument(Span {
                    start: 0,
                    end: input.len(),
                })
                .unwrap_err();
            parsed.push(result);
        }
        insta::assert_debug_snapshot!(parsed);
    }

    #[test]
    fn test_parse_commands() {
        let inputs = vec![
            // Publish
            "--publish foo/bar",
            "--publish foo/bar.ptb",
            // Upgrade
            "--upgrade foo @0x1",
            // Transfer objects
            "--transfer-objects a [b, c]",
            "--transfer-objects a [b]",
            "--transfer-objects a.0 [b]",
            // Split coins
            "--split-coins a [b, c]",
            "--split-coins a [c]",
            // Merge coins
            "--merge-coins a [b, c]",
            "--merge-coins a [c]",
            // Assign
            "--assign a",
            "--assign a b.1",
            "--assign a 1u8",
            "--assign a @0x1",
            "--assign a vector[1, 2, 3]",
            "--assign a none",
            "--assign a some(1)",
            // Gas budget
            "--gas-budget 1",
            // Pick gas budget
            "--pick-gas-budget max",
            "--pick-gas-budget sum",
            // Gas-coin
            "--gas-coin @0x1",
            "--summary",
            "--json",
            "--preview",
            "--warn-shadows",
        ];
        let mut parsed = Vec::new();
        for input in inputs {
            let x = shlex::split(input).unwrap();
            let parser = ProgramParser::new(x.iter().map(|x| x.as_str())).unwrap();
            let result = parser.parse().unwrap();
            parsed.push(result);
        }
        insta::assert_debug_snapshot!(parsed);
    }

    #[test]
    fn test_parse_commands_invalid() {
        let inputs = vec![
            // Publish
            "--publish",
            // Upgrade
            "--upgrade",
            "--upgrade 1",
            // Transfer objects
            "--transfer-objects a",
            "--transfer-objects [b]",
            "--transfer-objects",
            // Split coins
            "--split-coins a",
            "--split-coins [b]",
            "--split-coins",
            "--split-coins a b c",
            "--split-coins a [b] c",
            // Merge coins
            "--merge-coins a",
            "--merge-coins [b]",
            "--merge-coins",
            "--merge-coins a b c",
            "--merge-coins a [b] c",
            // Assign
            "--assign",
            "--assign a b c",
            "--assign none b",
            "--assign some none",
            "--assign a.3 1u8",
            // Gas budget
            "--gas-budget 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
            "--gas-budget [1]",
            "--gas-budget @0x1",
            "--gas-budget woah",
            // Pick gas budget
            "--pick-gas-budget nope",
            "--pick-gas-budget",
            "--pick-gas-budget too many",
            // Gas-coin
            "--gas-coin nope",
            "--gas-coin",
            "--gas-coin @0x1 @0x2",
            "--gas-coin 1",
        ];
        let mut parsed = Vec::new();
        for input in inputs {
            let x = shlex::split(input).unwrap();
            let parser = ProgramParser::new(x.iter().map(|x| x.as_str())).unwrap();
            let result = parser.parse().unwrap_err();
            parsed.push(result);
        }
        insta::assert_debug_snapshot!(parsed);
    }
}
