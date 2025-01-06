// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::iter::Peekable;

use move_core_types::parsing::{
    address::{NumericalAddress, ParsedAddress},
    parser::{parse_u128, parse_u16, parse_u256, parse_u32, parse_u64, parse_u8},
    types::{ParsedFqName, ParsedModuleId, ParsedStructType, ParsedType},
};
use sui_types::{base_types::ObjectID, Identifier};

use crate::{
    client_ptb::{
        ast::{all_keywords, COMMANDS},
        builder::{display_did_you_mean, find_did_you_means},
    },
    err, error, sp,
};

use super::{
    ast::{self as A, is_keyword, Argument, ModuleAccess, ParsedPTBCommand, ParsedProgram},
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
    serialize_unsigned_set: bool,
    serialize_signed_set: bool,
    json_set: bool,
    dry_run_set: bool,
    dev_inspect_set: bool,
    gas_object_id: Option<Spanned<ObjectID>>,
    gas_budget: Option<Spanned<u64>>,
}

impl<'a, I: Iterator<Item = &'a str>> ProgramParser<'a, I> {
    /// Create a PTB program parser from a sequence of string.
    pub fn new(tokens: I) -> PTBResult<Self> {
        let Some(tokens) = Lexer::new(tokens) else {
            error!(Span { start: 0, end: 0 }, "No tokens")
        };
        Ok(Self {
            tokens: tokens.peekable(),
            state: ProgramParsingState {
                parsed: Vec::new(),
                errors: Vec::new(),
                preview_set: false,
                summary_set: false,
                warn_shadows_set: false,
                serialize_unsigned_set: false,
                serialize_signed_set: false,
                json_set: false,
                dry_run_set: false,
                dev_inspect_set: false,
                gas_object_id: None,
                gas_budget: None,
            },
        })
    }

    /// Parse the sequence of strings into a PTB program. We continue to parse even if an error is
    /// raised, and return the errors at the end. If no errors are raised, we return the parsed PTB
    /// program along with the metadata that we have parsed (e.g., json output, summary).
    pub fn parse(mut self) -> Result<ParsedProgram, Vec<PTBError>> {
        use Lexeme as L;
        use Token as T;

        while let Some(sp!(sp, lexeme)) = self.tokens.next() {
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
                    let sp!(sp_args, value) = try_!($args);
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
                L(T::Command, A::SERIALIZE_UNSIGNED) => flag!(serialize_unsigned_set),
                L(T::Command, A::SERIALIZE_SIGNED) => flag!(serialize_signed_set),
                L(T::Command, A::SUMMARY) => flag!(summary_set),
                L(T::Command, A::JSON) => flag!(json_set),
                L(T::Command, A::DRY_RUN) => flag!(dry_run_set),
                L(T::Command, A::DEV_INSPECT) => flag!(dev_inspect_set),
                L(T::Command, A::PREVIEW) => flag!(preview_set),
                L(T::Command, A::WARN_SHADOWS) => flag!(warn_shadows_set),
                L(T::Command, A::GAS_COIN) => {
                    let specifier = try_!(self.parse_gas_specifier());
                    self.state.gas_object_id = Some(specifier);
                }
                L(T::Command, A::GAS_BUDGET) => {
                    let budget = try_!(self.parse_gas_budget()).widen_span(sp);
                    if let Some(other) = self.state.gas_budget.replace(budget) {
                        self.state.errors.extend([
                            err!(
                                other.span,
                                "Multiple gas budgets found. Gas budget first set here.",
                            ),
                            err!(budget.span => help: {
                                "PTBs must have exactly one gas budget set."
                            },"Budget set again here."),
                        ]);
                        self.fast_forward_to_next_command();
                    }
                }

                L(T::Command, A::TRANSFER_OBJECTS) => command!(self.parse_transfer_objects()),
                L(T::Command, A::SPLIT_COINS) => command!(self.parse_split_coins()),
                L(T::Command, A::MERGE_COINS) => command!(self.parse_merge_coins()),
                L(T::Command, A::ASSIGN) => command!(self.parse_assign()),
                L(T::Command, A::MAKE_MOVE_VEC) => command!(self.parse_make_move_vec()),
                L(T::Command, A::MOVE_CALL) => command!(self.parse_move_call()),

                L(T::Publish, src) => command!({
                    let src = sp.wrap(src.to_owned());
                    Ok(sp.wrap(ParsedPTBCommand::Publish(src)))
                }),

                L(T::Upgrade, src) => command!({
                    let src = sp.wrap(src.to_owned());
                    let cap = try_!(self.parse_argument());
                    Ok(cap.span.wrap(ParsedPTBCommand::Upgrade(src, cap)))
                }),

                L(T::Command, s) => {
                    let possibles = find_did_you_means(s, COMMANDS.iter().copied())
                        .into_iter()
                        .map(|s| format!("--{s}"))
                        .collect();
                    let err = if let Some(suggestion) = display_did_you_mean(possibles) {
                        err!(
                            sp => help: { "{suggestion}" },
                            "Unknown {lexeme}",
                        )
                    } else {
                        err!(sp, "Unknown {lexeme}")
                    };
                    self.state.errors.push(err);
                    self.fast_forward_to_next_command();
                }

                L(T::Eof, _) => break,

                unexpected => {
                    let err = err!(
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

        let sp!(sp, tok) = self.peek();

        if !tok.is_terminal() {
            self.state
                .errors
                .push(err!(sp, "Trailing {tok} found after the last command",));
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
                    serialize_unsigned_set: self.state.serialize_unsigned_set,
                    serialize_signed_set: self.state.serialize_signed_set,
                    gas_object_id: self.state.gas_object_id,
                    json_set: self.state.json_set,
                    dry_run_set: self.state.dry_run_set,
                    dev_inspect_set: self.state.dev_inspect_set,
                    gas_budget: self.state.gas_budget,
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
        let result @ sp!(sp, lexeme@Lexeme(token, _)) = self.peek();
        Ok(if token == expected {
            self.bump();
            result
        } else {
            error!(sp, "Expected {expected} but found {lexeme}");
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
            let sp!(_, lexeme) = self.peek();
            if lexeme.is_command_end() {
                break;
            }
            self.bump();
        }
    }
}

/// Methods for parsing commands
impl<'a, I: Iterator<Item = &'a str>> ProgramParser<'a, I> {
    /// Parse a transfer-objects command.
    /// The expected format is: `--transfer-objects [<from>, ...] <to>`
    fn parse_transfer_objects(&mut self) -> PTBResult<Spanned<ParsedPTBCommand>> {
        let transfer_froms = self.parse_array()?;
        let transfer_to = self.parse_argument()?;
        let sp = transfer_to.span.widen(transfer_froms.span);
        Ok(sp.wrap(ParsedPTBCommand::TransferObjects(
            transfer_froms,
            transfer_to,
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
        use Lexeme as L;
        let sp!(sp, L(_, contents)) = self.expect(Token::Ident)?;
        if is_keyword(contents) {
            error!(sp => help: {
                "Variable names cannot be {}.",
                all_keywords()
            },
            "Expected a variable name but found reserved word '{contents}'.");
        }

        let ident = sp.wrap(contents.to_owned());

        Ok(if self.peek().value.is_command_end() {
            ident.span.wrap(ParsedPTBCommand::Assign(ident, None))
        } else {
            let value = self.parse_argument()?;
            let sp = ident.span.widen(value.span);
            sp.wrap(ParsedPTBCommand::Assign(ident, Some(value)))
        })
    }

    /// Parse a make-move-vec command
    /// The expected format is: `--make-move-vec <type> [<elem>, ...]`
    fn parse_make_move_vec(&mut self) -> PTBResult<Spanned<ParsedPTBCommand>> {
        use Token as T;

        let sp!(start_sp, _) = self.expect(T::LAngle)?;
        let type_ = self.parse_type()?;
        self.expect(T::RAngle)?;

        let elems = self.parse_array()?;

        let sp = start_sp.widen(elems.span);
        Ok(sp.wrap(ParsedPTBCommand::MakeMoveVec(type_, elems)))
    }

    /// Parse a move-call command
    /// The expected format is: `--move-call <address>::<module>::<name> (<<type>, ...>)? <arg> ...`
    fn parse_move_call(&mut self) -> PTBResult<Spanned<ParsedPTBCommand>> {
        use Lexeme as L;
        use Token as T;

        let function = self.parse_module_access()?;
        let mut end_sp = function.span;

        let ty_args = if let sp!(_, L(T::LAngle, _)) = self.peek() {
            let type_args = self.parse_type_args()?;
            end_sp = type_args.span;
            Some(type_args)
        } else {
            None
        };

        let mut args = vec![];
        while !self.peek().value.is_command_end() {
            let arg = self.parse_argument()?;
            end_sp = arg.span;
            args.push(arg);
        }

        let sp = function.span.widen(end_sp);
        Ok(sp.wrap(ParsedPTBCommand::MoveCall(function, ty_args, args)))
    }

    /// Parse a gas-budget command.
    /// The expected format is: `--gas-budget <u64>`
    fn parse_gas_budget(&mut self) -> PTBResult<Spanned<u64>> {
        Ok(match self.parse_argument()? {
            sp!(sp, Argument::U64(u)) => sp.wrap(u),
            sp!(sp, Argument::InferredNum(n)) => {
                sp.wrap(u64::try_from(n).map_err(|_| err!(sp, "Value does not fit within a u64"))?)
            }
            sp!(sp, _) => error!(sp, "Expected a u64 value"),
        })
    }

    /// Parse a gas specifier.
    /// The expected format is: `--gas-coin <address>`
    fn parse_gas_specifier(&mut self) -> PTBResult<Spanned<ObjectID>> {
        Ok(self
            .parse_address_literal()?
            .map(|a| ObjectID::from(a.into_inner())))
    }
}

/// Methods for parsing arguments and types in commands
impl<'a, I: Iterator<Item = &'a str>> ProgramParser<'a, I> {
    /// Parse a single PTB argument from the beginning of the token stream.
    fn parse_argument(&mut self) -> PTBResult<Spanned<Argument>> {
        use Argument as V;
        use Lexeme as L;
        use Token as T;

        let sp!(sp, lexeme) = self.peek();
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

            L(T::At, _) => self.parse_address_literal()?.map(V::Address),

            L(T::Ident, A::NONE) => {
                self.bump();
                sp.wrap(V::Option(sp.wrap(None)))
            }

            L(T::Ident, A::SOME) => {
                self.bump();
                self.expect(T::LParen)?;
                let sp!(arg_sp, arg) = self.parse_argument()?;
                let sp!(end_sp, _) = self.expect(T::RParen)?;

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

            unexpected => error!(
                sp => help: { "Expected an argument here" },
                "Unexpected {unexpected}",
            ),
        })
    }

    /// Parse a type.
    fn parse_type(&mut self) -> PTBResult<Spanned<ParsedType>> {
        use Lexeme as L;
        use Token as T;

        let sp!(sp, lexeme) = self.peek();

        macro_rules! type_ {
            ($ty: expr) => {{
                self.bump();
                sp.wrap($ty)
            }};
        }

        Ok(match lexeme {
            L(T::Ident, A::U8) => type_!(ParsedType::U8),
            L(T::Ident, A::U16) => type_!(ParsedType::U16),
            L(T::Ident, A::U32) => type_!(ParsedType::U32),
            L(T::Ident, A::U64) => type_!(ParsedType::U64),
            L(T::Ident, A::U128) => type_!(ParsedType::U128),
            L(T::Ident, A::U256) => type_!(ParsedType::U256),
            L(T::Ident, A::BOOL) => type_!(ParsedType::Bool),
            L(T::Ident, A::ADDRESS) => type_!(ParsedType::Address),

            L(T::Ident, A::VECTOR) => {
                self.bump();
                self.expect(T::LAngle)?;
                let sp!(_, ty) = self.parse_type()?;
                let sp!(end_sp, _) = self.expect(T::RAngle)?;

                let sp = sp.widen(end_sp);
                sp.wrap(ParsedType::Vector(Box::new(ty)))
            }

            L(T::Ident | T::Number | T::HexNumber, _) => 'fq: {
                let sp!(_, module_access) = self.parse_module_access()?;
                let sp!(_, address) = module_access.address;
                let sp!(_, module_name) = module_access.module_name;
                let sp!(fun_sp, function_name) = module_access.function_name;

                let module = ParsedModuleId {
                    address,
                    name: module_name.to_string(),
                };

                let name = function_name.to_string();
                let fq_name = ParsedFqName { module, name };

                let sp!(_, L(T::LAngle, _)) = self.peek() else {
                    let sp = sp.widen(fun_sp);
                    break 'fq sp.wrap(ParsedType::Struct(ParsedStructType {
                        fq_name,
                        type_args: vec![],
                    }));
                };

                let sp!(tys_sp, type_args) = self.parse_type_args()?;

                let sp = sp.widen(tys_sp);
                sp.wrap(ParsedType::Struct(ParsedStructType { fq_name, type_args }))
            }

            unexpected => error!(
                sp => help: { "Expected a type here" },
                "Unexpected {unexpected}",
            ),
        })
    }

    /// Parse a fully-qualified name, corresponding to accessing a function or type from a module.
    fn parse_module_access(&mut self) -> PTBResult<Spanned<ModuleAccess>> {
        use Lexeme as L;
        use Token as T;

        let address = self.parse_address()?;

        self.expect(T::ColonColon)?;
        let sp!(mod_sp, L(_, module_name)) = self.expect(T::Ident)?;
        let module_name = Identifier::new(module_name)
            .map_err(|_| err!(mod_sp, "Invalid module name {module_name:?}"))?;

        self.expect(T::ColonColon)?;
        let sp!(fun_sp, L(_, function_name)) = self.expect(T::Ident)?;
        let function_name = Identifier::new(function_name)
            .map_err(|_| err!(fun_sp, "Invalid function name {function_name:?}"))?;

        let sp = address.span.widen(fun_sp);
        Ok(sp.wrap(ModuleAccess {
            address,
            module_name: mod_sp.wrap(module_name),
            function_name: fun_sp.wrap(function_name),
        }))
    }

    /// Parse a list of type arguments, surrounded by angle brackets, and separated by commas.
    fn parse_type_args(&mut self) -> PTBResult<Spanned<Vec<ParsedType>>> {
        use Lexeme as L;
        use Token as T;

        let sp!(start_sp, _) = self.expect(T::LAngle)?;

        let mut type_args = vec![];
        loop {
            type_args.push(self.parse_type()?.value);

            let sp!(sp, lexeme) = self.peek();
            match lexeme {
                L(T::Comma, _) => self.bump(),
                L(T::RAngle, _) => break,
                unexpected => error!(
                    sp => help: { "Expected {} or {}", T::Comma, T::RAngle },
                    "Unexpected {unexpected}",
                ),
            }
        }

        let sp!(end_sp, _) = self.expect(T::RAngle)?;
        Ok(start_sp.widen(end_sp).wrap(type_args))
    }

    /// Parse a variable access (a non-empty chain of identifiers, separated by '.')
    fn parse_variable(&mut self) -> PTBResult<Spanned<Argument>> {
        use Lexeme as L;
        use Token as T;

        let sp!(start_sp, L(_, ident)) = self.expect(T::Ident)?;
        let ident = start_sp.wrap(ident.to_owned());

        let sp!(_, L(T::Dot, _)) = self.peek() else {
            return Ok(start_sp.wrap(Argument::Identifier(ident.value)));
        };

        self.bump();
        let mut fields = vec![];
        loop {
            // A field can be any non-terminal token (identifier, number, etc).
            let sp!(sp, lexeme@L(_, field)) = self.peek();
            if lexeme.is_terminal() {
                error!(sp, "Expected a field name after '.'");
            }

            self.bump();
            fields.push(sp.wrap(field.to_owned()));

            if let sp!(_, L(T::Dot, _)) = self.peek() {
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

        let sp!(sp, suffix) = self.peek();

        macro_rules! parse_num {
            ($fn: ident, $ty: expr) => {{
                self.bump();
                let sp = sp.widen(contents.span);
                match $fn(contents.value) {
                    Ok((value, _)) => sp.wrap($ty(value)),
                    Err(e) => error!(sp, "{e}"),
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

            // If there's no suffix, parse as `InferredNum`, and don't consume the peeked character.
            _ => match parse_u256(contents.value) {
                Ok((value, _)) => contents.span.wrap(V::InferredNum(value)),
                Err(_) => error!(contents.span, "Invalid integer literal"),
            },
        })
    }

    /// Parse a numerical or named address.
    fn parse_address(&mut self) -> PTBResult<Spanned<ParsedAddress>> {
        use Lexeme as L;
        use Token as T;

        let sp!(sp, lexeme) = self.peek();
        let addr = match lexeme {
            L(T::Ident, name) => {
                self.bump();
                ParsedAddress::Named(name.to_owned())
            }

            L(T::Number, number) => {
                self.bump();
                NumericalAddress::parse_str(number)
                    .map_err(|e| err!(sp, "Failed to parse address {number:?}: {e}"))
                    .map(ParsedAddress::Numerical)?
            }

            L(T::HexNumber, number) => {
                self.bump();
                let number = format!("0x{number}");
                NumericalAddress::parse_str(&number)
                    .map_err(|e| err!(sp, "Failed to parse address {number:?}: {e}"))
                    .map(ParsedAddress::Numerical)?
            }

            unexpected => error!(
                sp => help: {
                    "Value addresses can either be a variable in-scope, or a numerical address, \
                     e.g., 0xc0ffee"
                },
                "Unexpected {unexpected}",
            ),
        };

        Ok(sp.wrap(addr))
    }

    /// Parse a numeric address literal (must be prefixed by an `@` symbol).
    fn parse_address_literal(&mut self) -> PTBResult<Spanned<NumericalAddress>> {
        let sp!(sp, _) = self.expect(Token::At).map_err(|e| {
            err!(e.span => help: {
                "Addresses or object IDs require the character '@' in front"
            }, "Expected an address")
        })?;

        Ok(match self.parse_address()?.widen_span(sp) {
            sp!(sp, ParsedAddress::Numerical(n)) => sp.wrap(n),
            sp!(sp, ParsedAddress::Named(n)) => error!(
                sp,
                "Expected a numerical address but got a named address '{n}'",
            ),
        })
    }

    // Parse an array of arguments. Each element of the array is separated by a comma.
    fn parse_array(&mut self) -> PTBResult<Spanned<Vec<Spanned<Argument>>>> {
        use Lexeme as L;
        use Token as T;
        let sp!(start_sp, _) = self.expect(T::LBracket)?;

        let mut values = vec![];
        loop {
            let sp!(sp, lexeme) = self.peek();
            if lexeme.is_terminal() {
                error!(
                    sp => help: { "Expected an array here" },
                    "Unexpected {lexeme}"
                );
            } else if let L(T::RBracket, _) = lexeme {
                break;
            }

            values.push(self.parse_argument()?);

            let sp!(sp, lexeme) = self.peek();
            match lexeme {
                L(T::RBracket, _) => break,
                L(T::Comma, _) => self.bump(),
                unexpected => error!(
                    sp => help: { "Expected {} or {}", T::RBracket, T::Comma },
                    "Unexpected {unexpected}",
                ),
            }
        }

        let sp!(end_sp, _) = self.expect(T::RBracket)?;
        Ok(start_sp.widen(end_sp).wrap(values))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse() {
        let input = "--transfer-objects [b, c] a";
        let mut x = shlex::split(input).unwrap();
        x.push("--gas-budget 1".to_owned());
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
            let mut x = shlex::split(input).unwrap();
            x.push("--gas-budget 1".to_owned());
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
            let mut x = shlex::split(input).unwrap();
            x.push("--gas-budget 1".to_owned());
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
    fn test_parse_types() {
        let inputs = vec![
            // Primitives
            "u8",
            "u16",
            "u32",
            "u64",
            "u128",
            "u256",
            "bool",
            "address",
            "vector<u8>",
            // Structs
            "sui::object::ID",
            "0x2::object::UID",
            "3::staking_pool::StakedSui",
            // Generic types
            "0x2::coin::Coin<2::sui::SUI>",
            "sui::table::Table<sui::object::ID, vector<0x1::option::Option<u32>>>",
        ];
        let mut parsed = Vec::new();
        for input in inputs {
            let mut x = shlex::split(input).unwrap();
            x.push("--gas-budget 1".to_owned());
            let mut parser = ProgramParser::new(x.iter().map(|x| x.as_str())).unwrap();
            let result = parser
                .parse_type()
                .unwrap_or_else(|e| panic!("Failed on {input:?}: {e:?}"));
            parsed.push(result);
        }
        insta::assert_debug_snapshot!(parsed);
    }

    #[test]
    fn test_parse_types_invalid() {
        let inputs = vec![
            "signer",
            "not-an-identifier",
            "vector<u8, u16>",
            "foo::",
            "foo::bar",
            "foo::bar::Baz<",
            "foo::bar::Baz<u8",
            "foo::bar::Baz<u8,",
        ];
        let mut parsed = Vec::new();
        for input in inputs {
            let x = shlex::split(input).unwrap();
            let mut parser = ProgramParser::new(x.iter().map(|x| x.as_str())).unwrap();
            let result = parser.parse_type().unwrap_err();
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
            "--transfer-objects [b, c] a",
            "--transfer-objects [b] a",
            "--transfer-objects [b] a.0",
            // Split coins
            "--split-coins a [b, c]",
            "--split-coins a [c]",
            // Merge coins
            "--merge-coins a [b, c]",
            "--merge-coins a [c]",
            // Make Move Vec
            "--make-move-vec <u64> []",
            "--make-move-vec <u8> [1u8, 2u8]",
            // Move Call
            "--move-call 0x3::sui_system::request_add_stake system coins.0 validator",
            "--move-call std::option::is_none <u64> p",
            "--move-call std::option::is_some<u32> q",
            // Assign
            "--assign a",
            "--assign a b.1",
            "--assign a 1u8",
            "--assign a @0x1",
            "--assign a vector[1, 2, 3]",
            "--assign a none",
            "--assign a some(1)",
            // Gas-coin
            "--gas-coin @0x1",
            "--summary",
            "--json",
            "--preview",
            "--warn-shadows",
        ];
        let mut parsed = Vec::new();
        for input in inputs {
            let mut x = shlex::split(input).unwrap();
            x.push("--gas-budget 1".to_owned());
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
            "--transfer-objects [a] [b]",
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
            // Make Move Vec
            "--make-move-vec",
            "--make-move-vec [1, 2, 3]",
            "--make-move-vec <u64>",
            // Move Call
            "--move-call",
            "--move-call 0x1::option::is_none<u8> []",
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
