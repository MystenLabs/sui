// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::iter::Peekable;

use move_command_line_common::{
    address::{NumericalAddress, ParsedAddress},
    parser::{parse_u128, parse_u16, parse_u256, parse_u32, parse_u64, parse_u8},
};
use sui_types::base_types::ObjectID;

use crate::{client_ptb::ast::GasPicker, err_, error_, sp_};

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
        use Lexeme as L;
        use Token as T;

        while let Some(sp_!(sp, lexeme)) = self.tokens.next() {
            macro_rules! try_ {
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

            macro_rules! command {
                ($args:expr) => {{
                    let sp_!(sp_args, value) = try_!($args);
                    let cmd = sp.widen(sp_args).wrap(value);
                    self.state.parsed.push(cmd);
                }};
            }

            macro_rules! flag {
                ($flag:ident) => {{
                    self.state.$flag = true;
                }};
            }

            match lexeme {
                L(T::Command, A::SUMMARY) => flag!(summary_set),
                L(T::Command, A::JSON) => flag!(json_set),
                L(T::Command, A::PREVIEW) => flag!(preview_set),
                L(T::Command, A::WARN_SHADOWS) => flag!(warn_shadows_set),
                L(T::Command, A::GAS_COIN) => {
                    let specifier = try_!(self.parse_gas_specifier());
                    self.state.gas_object_id = Some(specifier);
                }

                L(T::Command, A::TRANSFER_OBJECTS) => command!(self.parse_transfer_objects()),
                L(T::Command, A::SPLIT_COINS) => command!(self.parse_split_coins()),
                L(T::Command, A::MERGE_COINS) => command!(self.parse_merge_coins()),
                L(T::Command, A::ASSIGN) => command!(self.parse_assign()),
                L(T::Command, A::GAS_BUDGET) => command!(self.parse_gas_budget()),
                L(T::Command, A::PICK_GAS_BUDGET) => command!(self.parse_pick_gas_budget()),
                L(T::Command, A::MAKE_MOVE_VEC) => todo!(),
                L(T::Command, A::MOVE_CALL) => todo!(),

                L(T::Publish, src) => command!({
                    let src = sp.wrap(src.to_owned());
                    Ok(sp.wrap(ParsedPTBCommand::Publish(src)))
                }),

                L(T::Upgrade, src) => command!({
                    let src = sp.wrap(src.to_owned());
                    let cap = try_!(self.parse_argument());
                    Ok(cap.span.wrap(ParsedPTBCommand::Upgrade(src, cap)))
                }),

                L(T::Command, _) => {
                    let err = err_!(sp, "Unknown {lexeme}");
                    self.state.errors.push(err);
                    self.fast_forward_to_next_command();
                }

                L(T::Eof, _) => break,

                unexpected => {
                    let err = err_!(
                        sp => help: { "Expected to find a command here" },
                        "Unexpected {unexpected}",
                    );

                    self.state.errors.push(err);
                    if unexpected.is_terminal() {
                        break;
                    } else {
                        self.fast_forward_to_next_command();
                    }
                }
            }
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
    /// Advance the iterator and return the next lexeme. If the next lexeme's token is not the
    /// expected one, return an error, and don't advance the token stream.
    fn expect(&mut self, expected: Token) -> PTBResult<Spanned<Lexeme<'a>>> {
        let result @ sp_!(sp, lexeme@Lexeme(token, _)) = self.peek();
        Ok(if token == expected {
            self.bump();
            result
        } else {
            error_!(sp, "Expected {expected} but found {lexeme}");
        })
    }

    /// Peek at the next token without advancing the iterator.
    fn peek(&mut self) -> Spanned<Lexeme<'a>> {
        *self
            .tokens
            .peek()
            .expect("Lexer returns an infinite stream")
    }

    /// Unconditionally advance the next token. It is always safe to do this, because the underlying
    /// token stream is "infinite" (the lexer will repeatedly return its terminal token).
    fn bump(&mut self) {
        self.tokens.next();
    }

    /// Fast forward to the next command token (if any).
    fn fast_forward_to_next_command(&mut self) {
        loop {
            let sp_!(_, lexeme) = self.peek();
            if lexeme.is_command_end() {
                break;
            } else {
                self.bump();
            }
        }
    }
}

/// Methods for parsing commands
impl<'a, I: Iterator<Item = &'a str>> ProgramParser<'a, I> {
    /// Parse a transfer-objects command.
    /// The expected format is: `--transfer-objects <to> [<from>, ...]`
    fn parse_transfer_objects(&mut self) -> PTBResult<Spanned<ParsedPTBCommand>> {
        let transfer_to = self.parse_argument()?;
        let transfer_froms = self.parse_array()?;
        let sp = transfer_to.span.widen(transfer_froms.span);
        Ok(sp.wrap(ParsedPTBCommand::TransferObjects(
            transfer_to,
            transfer_froms,
        )))
    }

    /// Parse a split-coins command.
    /// The expected format is: `--split-coins <coin> [<amount>, ...]`
    fn parse_split_coins(&mut self) -> PTBResult<Spanned<ParsedPTBCommand>> {
        let split_from = self.parse_argument()?;
        let splits = self.parse_array()?;
        let sp = split_from.span.widen(splits.span);
        Ok(sp.wrap(ParsedPTBCommand::SplitCoins(split_from, splits)))
    }

    /// Parse a merge-coins command.
    /// The expected format is: `--merge-coins <coin> [<coin1>, ...]`
    fn parse_merge_coins(&mut self) -> PTBResult<Spanned<ParsedPTBCommand>> {
        let merge_into = self.parse_argument()?;
        let coins = self.parse_array()?;
        let sp = merge_into.span.widen(coins.span);
        Ok(sp.wrap(ParsedPTBCommand::MergeCoins(merge_into, coins)))
    }

    /// Parse an assign command.
    /// The expected format is: `--assign <variable> (<value>)?`
    fn parse_assign(&mut self) -> PTBResult<Spanned<ParsedPTBCommand>> {
        let ident = match self.parse_argument()? {
            sp_!(sp, Argument::Identifier(i)) => sp.wrap(i),
            sp_!(sp, _) => error_!(sp, "Expected variable binding"),
        };

        Ok(if self.peek().value.is_command_end() {
            ident.span.wrap(ParsedPTBCommand::Assign(ident, None))
        } else {
            let value = self.parse_argument()?;
            let sp = ident.span.widen(value.span);
            sp.wrap(ParsedPTBCommand::Assign(ident, Some(value)))
        })
    }

    /// Parse a pick-gas-budget command.
    /// The expected format is: `--pick-gas-budget <picker>` where picker is either `max` or `sum`.
    fn parse_pick_gas_budget(&mut self) -> PTBResult<Spanned<ParsedPTBCommand>> {
        use Lexeme as L;
        use Token as T;

        let sp_!(sp, lexeme) = self.peek();
        let picker = match lexeme {
            L(T::Ident, "max") => {
                self.bump();
                sp.wrap(GasPicker::Max)
            }

            L(T::Ident, "sum") => {
                self.bump();
                sp.wrap(GasPicker::Sum)
            }

            unexpected => error_!(
                sp => help: { "Expected 'max' or 'sum' here" },
                "Unexpected {unexpected}",
            ),
        };

        Ok(sp.wrap(ParsedPTBCommand::PickGasBudget(picker)))
    }

    /// Parse a gas-budget command.
    /// The expected format is: `--gas-budget <u64>`
    fn parse_gas_budget(&mut self) -> PTBResult<Spanned<ParsedPTBCommand>> {
        Ok(match self.parse_argument()? {
            sp_!(sp, Argument::U64(u)) => sp.wrap(ParsedPTBCommand::GasBudget(sp.wrap(u))),
            sp_!(sp, _) => error_!(sp, "Expected a u64 value"),
        })
    }

    /// Parse a gas specifier.
    /// The expected format is: `--gas-coin <address>`
    fn parse_gas_specifier(&mut self) -> PTBResult<Spanned<ObjectID>> {
        Ok(match self.parse_argument()? {
            sp_!(sp, Argument::Address(a)) => sp.wrap(ObjectID::from(a.into_inner())),
            sp_!(sp, _) => error_!(sp, "Expected an address"),
        })
    }
}

/// Methods for parsing arguments and types in commands
impl<'a, I: Iterator<Item = &'a str>> ProgramParser<'a, I> {
    /// Parse a single PTB argument from the beginning of the token stream.
    fn parse_argument(&mut self) -> PTBResult<Spanned<Argument>> {
        use Argument as V;
        use Lexeme as L;
        use Token as T;

        let sp_!(sp, lexeme) = self.peek();
        Ok(match lexeme {
            L(T::Ident, "true") => {
                self.bump();
                sp.wrap(V::Bool(true))
            }

            L(T::Ident, "false") => {
                self.bump();
                sp.wrap(V::Bool(false))
            }

            L(T::Ident, A::GAS) => {
                self.bump();
                sp.wrap(V::Gas)
            }

            L(T::Number | T::HexNumber, number) => {
                let number = if lexeme.0 == T::HexNumber {
                    format!("0x{number}")
                } else {
                    number.to_owned()
                };

                self.bump();
                self.parse_number(sp.wrap(&number))?
            }

            L(T::At, _) => {
                self.bump();
                match self.parse_address()?.widen_span(sp) {
                    sp_!(sp, ParsedAddress::Numerical(n)) => sp.wrap(V::Address(n)),
                    sp_!(sp, ParsedAddress::Named(n)) => error_!(
                        sp,
                        "Expected a numerical address but got a named address '{n}'",
                    ),
                }
            }

            L(T::Ident, A::NONE) => {
                self.bump();
                sp.wrap(V::Option(sp.wrap(None)))
            }

            L(T::Ident, A::SOME) => {
                self.bump();
                self.expect(T::LParen)?;
                let sp_!(arg_sp, arg) = self.parse_argument()?;
                let sp_!(end_sp, _) = self.expect(T::RParen)?;

                let sp = sp.widen(end_sp);
                sp.wrap(V::Option(arg_sp.wrap(Some(Box::new(arg)))))
            }

            L(T::Ident, A::VECTOR) => {
                self.bump();
                self.parse_array()?.map(V::Vector).widen_span(sp)
            }

            L(T::Ident, _) => self.parse_variable()?,

            L(T::String, contents) => {
                self.bump();
                sp.wrap(V::String(contents.to_owned()))
            }

            unexpected => error_!(
                sp => help: { "Expected an argument here" },
                "Unexpected {unexpected}",
            ),
        })
    }

    /// Parse a variable access (a non-empty chain of identifiers, separated by '.')
    fn parse_variable(&mut self) -> PTBResult<Spanned<Argument>> {
        use Lexeme as L;
        use Token as T;

        let sp_!(start_sp, L(_, ident)) = self.expect(T::Ident)?;
        let ident = start_sp.wrap(ident.to_owned());

        let sp_!(_, L(T::Dot, _)) = self.peek() else {
            return Ok(start_sp.wrap(Argument::Identifier(ident.value)));
        };

        self.bump();
        let mut fields = vec![];
        loop {
            // A field can be any non-terminal token (identifier, number, etc).
            let sp_!(sp, lexeme@L(_, field)) = self.peek();
            if lexeme.is_terminal() {
                error_!(sp, "Expected a field name after '.'");
            } else {
                self.bump();
                fields.push(sp.wrap(field.to_owned()));
            }

            if let sp_!(_, L(T::Dot, _)) = self.peek() {
                self.bump();
            } else {
                break;
            }
        }

        let end_sp = fields.last().map(|f| f.span).unwrap_or(start_sp);
        let sp = start_sp.widen(end_sp);
        Ok(sp.wrap(Argument::VariableAccess(ident, fields)))
    }

    /// Parse a decimal or hexadecimal number, optionally followed by a type suffix.
    fn parse_number(&mut self, contents: Spanned<&str>) -> PTBResult<Spanned<Argument>> {
        use Argument as V;
        use Lexeme as L;
        use Token as T;

        let sp_!(sp, suffix) = self.peek();

        macro_rules! parse_num {
            ($fn: ident, $ty: expr) => {{
                self.bump();
                let sp = sp.widen(contents.span);
                match $fn(contents.value) {
                    Ok((value, _)) => sp.wrap($ty(value)),
                    Err(e) => error_!(sp, "{e}"),
                }
            }};
        }

        Ok(match suffix {
            L(T::Ident, A::U8) => parse_num!(parse_u8, V::U8),
            L(T::Ident, A::U16) => parse_num!(parse_u16, V::U16),
            L(T::Ident, A::U32) => parse_num!(parse_u32, V::U32),
            L(T::Ident, A::U64) => parse_num!(parse_u64, V::U64),
            L(T::Ident, A::U128) => parse_num!(parse_u128, V::U128),
            L(T::Ident, A::U256) => parse_num!(parse_u256, V::U256),

            // If there's no suffix, assume u64, and don't consume the peeked character.
            _ => match parse_u64(contents.value) {
                Ok((value, _)) => contents.span.wrap(V::U64(value)),
                Err(e) => error_!(contents.span, "{e}"),
            },
        })
    }

    /// Parse a numerical or named address.
    fn parse_address(&mut self) -> PTBResult<Spanned<ParsedAddress>> {
        use Lexeme as L;
        use Token as T;

        let sp_!(sp, lexeme) = self.peek();
        let addr = match lexeme {
            L(T::Ident, name) => {
                self.bump();
                ParsedAddress::Named(name.to_owned())
            }

            L(T::Number, number) => {
                self.bump();
                NumericalAddress::parse_str(number)
                    .map_err(|e| err_!(sp, "Failed to parse address {number:?}: {e}"))
                    .map(ParsedAddress::Numerical)?
            }

            L(T::HexNumber, number) => {
                self.bump();
                let number = format!("0x{number}");
                NumericalAddress::parse_str(&number)
                    .map_err(|e| err_!(sp, "Failed to parse address {number:?}: {e}"))
                    .map(ParsedAddress::Numerical)?
            }

            unexpected => error_!(
                sp => help: {
                    "Value addresses can either be a variable in-scope, or a numerical address, \
                     e.g., 0xc0ffee"
                },
                "Unexpected {unexpected}",
            ),
        };

        Ok(sp.wrap(addr))
    }

    // Parse an array of arguments. Each element of the array is separated by a comma.
    fn parse_array(&mut self) -> PTBResult<Spanned<Vec<Spanned<Argument>>>> {
        use Lexeme as L;
        use Token as T;
        let sp_!(start_sp, _) = self.expect(T::LBracket)?;

        let mut values = vec![];
        loop {
            let sp_!(sp, lexeme) = self.peek();
            if lexeme.is_terminal() {
                error_!(
                    sp => help: { "Expected an array here" },
                    "Unexpected {lexeme}"
                );
            } else if let L(T::RBracket, _) = lexeme {
                break;
            }

            values.push(self.parse_argument()?);

            let sp_!(sp, lexeme) = self.peek();
            match lexeme {
                L(T::RBracket, _) => break,
                L(T::Comma, _) => self.bump(),
                unexpected => error_!(
                    sp => help: { "Expected {} or {}", T::RBracket, T::Comma },
                    "Unexpected {unexpected}",
                ),
            }
        }

        let sp_!(end_sp, _) = self.expect(T::RBracket)?;
        Ok(start_sp.widen(end_sp).wrap(values))
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
                .parse_argument()
                .unwrap_or_else(|e| panic!("Failed on {input:?}: {e:?}"));
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
            let result = parser.parse_argument().unwrap_err();
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
            let result = parser
                .parse()
                .unwrap_or_else(|e| panic!("Failed on {input:?}: {e:?}"));
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
