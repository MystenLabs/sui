// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{borrow::BorrowMut, marker::PhantomData, str::FromStr};

use move_command_line_common::parser::{Parser, Token};
use move_core_types::{
    account_address::AccountAddress, identifier::Identifier, parsing::parse_type_tags_with_resolver,
};
use sui_types::{
    base_types::ObjectID,
    transaction::{Argument, Command, ProgrammableMoveCall},
    type_input::TypeInput,
    TypeTag,
};

use crate::programmable_transaction_test_parser::token::{
    GAS_COIN, INPUT, MAKE_MOVE_VEC, MERGE_COINS, NESTED_RESULT, PUBLISH, RESULT, SPLIT_COINS,
    TRANSFER_OBJECTS, UPGRADE,
};

use super::token::CommandToken;
use anyhow::{bail, Context, Result};

/// A small parser used for parsing programmable transaction commands for transactional tests
pub struct CommandParser<
    'a,
    I: Iterator<Item = (CommandToken, &'a str)>,
    P: BorrowMut<Parser<'a, CommandToken, I>>,
    R: Fn(&str) -> Option<AccountAddress>,
> {
    inner: P,
    resolver: &'a R,
    _a: PhantomData<&'a ()>,
    _i: PhantomData<I>,
}

#[derive(Debug, Clone)]
pub struct ParsedMoveCall {
    pub package: Identifier,
    pub module: Identifier,
    pub function: Identifier,
    pub type_arguments: Vec<TypeTag>,
    pub arguments: Vec<Argument>,
}

#[derive(Debug, Clone)]
pub enum ParsedCommand {
    MoveCall(Box<ParsedMoveCall>),
    TransferObjects(Vec<Argument>, Argument),
    SplitCoins(Argument, Vec<Argument>),
    MergeCoins(Argument, Vec<Argument>),
    MakeMoveVec(Option<TypeTag>, Vec<Argument>),
    Publish(String, Vec<String>),
    Upgrade(String, Vec<String>, String, Argument),
}

impl<'a, I, R> CommandParser<'a, I, Parser<'a, CommandToken, I>, R>
where
    I: Iterator<Item = (CommandToken, &'a str)>,
    R: Fn(&str) -> Option<AccountAddress>,
{
    pub fn new<T: IntoIterator<Item = (CommandToken, &'a str), IntoIter = I>>(
        v: T,
        resolver: &'a R,
    ) -> Self {
        Self::from_parser(Parser::new(v), resolver)
    }
}

impl<'a, I, P, R> CommandParser<'a, I, P, R>
where
    I: Iterator<Item = (CommandToken, &'a str)>,
    P: BorrowMut<Parser<'a, CommandToken, I>>,
    R: Fn(&str) -> Option<AccountAddress>,
{
    pub fn from_parser(inner: P, resolver: &'a R) -> Self {
        Self {
            inner,
            resolver,
            _a: PhantomData,
            _i: PhantomData,
        }
    }

    pub fn parse_commands(&mut self) -> Result<Vec<ParsedCommand>> {
        let commands = self.inner.borrow_mut().parse_list(
            |p| CommandParser::from_parser(p, self.resolver).parse_command_start(),
            CommandToken::Semi,
            /* not checked */ CommandToken::Void,
            /* allow_trailing_delim */ true,
        )?;
        let commands = commands
            .into_iter()
            .enumerate()
            .map(|(actual, (annotated, c))| {
                if let Some(annotated) = annotated {
                    if actual != annotated {
                        anyhow::bail!(
                            "Actual command index of {actual} \
                            does not match annotated index {annotated}",
                        );
                    }
                }
                Ok(c)
            })
            .collect::<Result<_>>()?;
        Ok(commands)
    }

    pub fn parse_command_start(&mut self) -> Result<(Option<usize>, ParsedCommand)> {
        self.inner
            .borrow_mut()
            .advance(CommandToken::CommandStart)?;
        let idx = if let Some(CommandToken::Number) = self.inner.borrow_mut().peek_tok() {
            let num = self.inner.borrow_mut().advance(CommandToken::Number)?;
            let idx = usize::from_str(num).context("Invalid command index annotation")?;
            self.inner.borrow_mut().advance(CommandToken::Colon)?;
            Some(idx)
        } else {
            None
        };
        let cmd = self.parse_command()?;
        Ok((idx, cmd))
    }

    pub fn parse_command(&mut self) -> Result<ParsedCommand> {
        use super::token::CommandToken as Tok;
        Ok(match self.inner.borrow_mut().advance_any()? {
            (Tok::Ident, TRANSFER_OBJECTS) => {
                self.inner.borrow_mut().advance(Tok::LParen)?;
                let args = self.parse_command_args(Tok::LBracket, Tok::RBracket)?;
                self.inner.borrow_mut().advance(Tok::Comma)?;
                let arg = self.parse_command_arg()?;
                self.maybe_trailing_comma()?;
                self.inner.borrow_mut().advance(Tok::RParen)?;
                ParsedCommand::TransferObjects(args, arg)
            }
            (Tok::Ident, SPLIT_COINS) => {
                self.inner.borrow_mut().advance(Tok::LParen)?;
                let coin = self.parse_command_arg()?;
                self.inner.borrow_mut().advance(Tok::Comma)?;
                let amts = self.parse_command_args(Tok::LBracket, Tok::RBracket)?;
                self.maybe_trailing_comma()?;
                self.inner.borrow_mut().advance(Tok::RParen)?;
                ParsedCommand::SplitCoins(coin, amts)
            }
            (Tok::Ident, MERGE_COINS) => {
                self.inner.borrow_mut().advance(Tok::LParen)?;
                let target = self.parse_command_arg()?;
                self.inner.borrow_mut().advance(Tok::Comma)?;
                let coins = self.parse_command_args(Tok::LBracket, Tok::RBracket)?;
                self.maybe_trailing_comma()?;
                self.inner.borrow_mut().advance(Tok::RParen)?;
                ParsedCommand::MergeCoins(target, coins)
            }
            (Tok::Ident, MAKE_MOVE_VEC) => {
                let type_opt = self.parse_ty_arg_opt()?;
                self.inner.borrow_mut().advance(Tok::LParen)?;
                let args = self.parse_command_args(Tok::LBracket, Tok::RBracket)?;
                self.maybe_trailing_comma()?;
                self.inner.borrow_mut().advance(Tok::RParen)?;
                ParsedCommand::MakeMoveVec(type_opt, args)
            }
            (Tok::Ident, PUBLISH) => {
                self.inner.borrow_mut().advance(Tok::LParen)?;
                let staged_package = self.inner.borrow_mut().advance(Tok::Ident)?;
                self.inner.borrow_mut().advance(Tok::Comma)?;
                self.inner.borrow_mut().advance(Tok::LBracket)?;
                let dependencies = self.inner.borrow_mut().parse_list(
                    |p| Ok(p.advance(Tok::Ident)?.to_owned()),
                    CommandToken::Comma,
                    Tok::RBracket,
                    /* allow_trailing_delim */ true,
                )?;
                self.inner.borrow_mut().advance(Tok::RBracket)?;
                self.maybe_trailing_comma()?;
                self.inner.borrow_mut().advance(Tok::RParen)?;
                ParsedCommand::Publish(staged_package.to_owned(), dependencies)
            }
            (Tok::Ident, UPGRADE) => {
                self.inner.borrow_mut().advance(Tok::LParen)?;
                let staged_package = self.inner.borrow_mut().advance(Tok::Ident)?;
                self.inner.borrow_mut().advance(Tok::Comma)?;
                self.inner.borrow_mut().advance(Tok::LBracket)?;
                let dependencies = self.inner.borrow_mut().parse_list(
                    |p| Ok(p.advance(Tok::Ident)?.to_owned()),
                    CommandToken::Comma,
                    Tok::RBracket,
                    /* allow_trailing_delim */ true,
                )?;
                self.inner.borrow_mut().advance(Tok::RBracket)?;
                self.inner.borrow_mut().advance(Tok::Comma)?;
                let upgraded_package = self.inner.borrow_mut().advance(Tok::Ident)?;
                self.inner.borrow_mut().advance(Tok::Comma)?;
                let upgrade_ticket = self.parse_command_arg()?;
                self.maybe_trailing_comma()?;
                self.inner.borrow_mut().advance(Tok::RParen)?;
                ParsedCommand::Upgrade(
                    staged_package.to_owned(),
                    dependencies,
                    upgraded_package.to_owned(),
                    upgrade_ticket,
                )
            }
            (Tok::Ident, contents) => {
                let package = Identifier::new(contents)?;
                self.inner.borrow_mut().advance(Tok::ColonColon)?;
                let module = Identifier::new(self.inner.borrow_mut().advance(Tok::Ident)?)?;
                self.inner.borrow_mut().advance(Tok::ColonColon)?;
                let function = Identifier::new(self.inner.borrow_mut().advance(Tok::Ident)?)?;
                let type_arguments = self.split_ty_args_opt()?.unwrap_or_default();
                let arguments = self.parse_command_args(Tok::LParen, Tok::RParen)?;
                let call = ParsedMoveCall {
                    package,
                    module,
                    function,
                    type_arguments,
                    arguments,
                };
                ParsedCommand::MoveCall(Box::new(call))
            }

            (tok, _) => bail!("unexpected token {}, expected command identifier", tok),
        })
    }

    pub fn maybe_trailing_comma(&mut self) -> Result<()> {
        if let Some(CommandToken::Comma) = self.inner.borrow_mut().peek_tok() {
            self.inner.borrow_mut().advance(CommandToken::Comma)?;
        }
        Ok(())
    }

    pub fn parse_command_args(
        &mut self,
        start: CommandToken,
        end: CommandToken,
    ) -> Result<Vec<Argument>> {
        self.inner.borrow_mut().advance(start)?;
        let args = self.inner.borrow_mut().parse_list(
            |p| CommandParser::from_parser(p, self.resolver).parse_command_arg(),
            CommandToken::Comma,
            end,
            /* allow_trailing_delim */ true,
        )?;
        self.inner.borrow_mut().advance(end)?;
        Ok(args)
    }

    pub fn parse_command_arg(&mut self) -> Result<Argument> {
        use super::token::CommandToken as Tok;
        Ok(match self.inner.borrow_mut().advance_any()? {
            (Tok::Ident, GAS_COIN) => Argument::GasCoin,
            (Tok::Ident, INPUT) => {
                self.inner.borrow_mut().advance(Tok::LParen)?;
                let num = self.parse_u16()?;
                self.maybe_trailing_comma()?;
                self.inner.borrow_mut().advance(Tok::RParen)?;
                Argument::Input(num)
            }
            (Tok::Ident, RESULT) => {
                self.inner.borrow_mut().advance(Tok::LParen)?;
                let num = self.parse_u16()?;
                self.maybe_trailing_comma()?;
                self.inner.borrow_mut().advance(Tok::RParen)?;
                Argument::Result(num)
            }
            (Tok::Ident, NESTED_RESULT) => {
                self.inner.borrow_mut().advance(Tok::LParen)?;
                let i = self.parse_u16()?;
                self.inner.borrow_mut().advance(Tok::Comma)?;
                let j = self.parse_u16()?;
                self.maybe_trailing_comma()?;
                self.inner.borrow_mut().advance(Tok::RParen)?;
                Argument::NestedResult(i, j)
            }
            (tok, _) => bail!("unexpected token {}, expected argument identifier", tok),
        })
    }

    pub fn parse_u16(&mut self) -> Result<u16> {
        let contents = self.inner.borrow_mut().advance(CommandToken::Number)?;
        u16::from_str(contents).context("Expected u16 for Argument")
    }

    pub fn parse_ty_arg_opt(&mut self) -> Result<Option<TypeTag>> {
        match self.split_ty_args_opt()? {
            None => Ok(None),
            Some(v) if v.len() != 1 => bail!(
                "unexpected multiple type arguments. Expected 1 type argument but got {}",
                v.len()
            ),
            Some(mut v) => Ok(Some(v.pop().unwrap())),
        }
    }

    pub fn split_ty_args_opt(&mut self) -> Result<Option<Vec<TypeTag>>> {
        if !matches!(
            self.inner.borrow_mut().peek_tok(),
            Some(CommandToken::TypeArgString)
        ) {
            return Ok(None);
        }
        let contents = self
            .inner
            .borrow_mut()
            .advance(CommandToken::TypeArgString)?;

        let type_args =
            parse_type_tags_with_resolver(contents, Some("<"), ",", ">", true, self.resolver)?;
        Ok(Some(type_args))
    }
}

impl ParsedCommand {
    pub fn parse_vec(
        s: &str,
        resolver: &impl Fn(&str) -> Option<AccountAddress>,
    ) -> Result<Vec<Self>> {
        let tokens: Vec<_> = CommandToken::tokenize(s)?
            .into_iter()
            .filter(|(tok, _)| !tok.is_whitespace())
            .collect();
        let mut parser = CommandParser::new(tokens, resolver);
        let res = parser.parse_commands()?;
        if let Ok((_, contents)) = parser.inner.borrow_mut().advance_any() {
            bail!("Expected end of token stream. Got: {}", contents)
        }
        Ok(res)
    }

    pub fn into_command(
        self,
        staged_packages: &impl Fn(&str) -> Option<Vec<Vec<u8>>>,
        address_mapping: &impl Fn(&str) -> Option<AccountAddress>,
    ) -> Result<Command> {
        Ok(match self {
            ParsedCommand::MoveCall(c) => {
                Command::MoveCall(Box::new(c.into_move_call(address_mapping)?))
            }
            ParsedCommand::TransferObjects(objs, recipient) => {
                Command::TransferObjects(objs, recipient)
            }
            ParsedCommand::SplitCoins(coin, amts) => Command::SplitCoins(coin, amts),
            ParsedCommand::MergeCoins(target, coins) => Command::MergeCoins(target, coins),
            ParsedCommand::MakeMoveVec(ty_opt, args) => Command::make_move_vec(ty_opt, args),
            ParsedCommand::Publish(staged_package, dependencies) => {
                let Some(package_contents) = staged_packages(&staged_package) else {
                    bail!("No staged package '{staged_package}'");
                };
                let dependencies = dependencies
                    .into_iter()
                    .map(|d| match address_mapping(&d) {
                        Some(a) => Ok(a.into()),
                        None => bail!("Unbound dependency '{d}"),
                    })
                    .collect::<Result<Vec<ObjectID>>>()?;
                Command::Publish(package_contents, dependencies)
            }
            ParsedCommand::Upgrade(staged_package, dependencies, upgraded_package, ticket) => {
                let Some(package_contents) = staged_packages(&staged_package) else {
                    bail!("No staged package '{staged_package}'");
                };
                let dependencies = dependencies
                    .into_iter()
                    .map(|d| match address_mapping(&d) {
                        Some(a) => Ok(a.into()),
                        None => bail!("Unbound dependency '{d}"),
                    })
                    .collect::<Result<Vec<ObjectID>>>()?;
                let Some(upgraded_package) = address_mapping(&upgraded_package) else {
                    bail!("Unbound upgraded package '{upgraded_package}'");
                };
                let upgraded_package = upgraded_package.into();
                Command::Upgrade(package_contents, dependencies, upgraded_package, ticket)
            }
        })
    }
}

impl ParsedMoveCall {
    pub fn into_move_call(
        self,
        address_mapping: &impl Fn(&str) -> Option<AccountAddress>,
    ) -> Result<ProgrammableMoveCall> {
        let Self {
            package,
            module,
            function,
            type_arguments,
            arguments,
        } = self;
        let Some(package) = address_mapping(package.as_str()) else {
            bail!("Unable to resolve package {}", package)
        };
        let type_arguments = type_arguments.into_iter().map(TypeInput::from).collect();
        Ok(ProgrammableMoveCall {
            package: package.into(),
            module: module.to_string(),
            function: function.to_string(),
            type_arguments,
            arguments,
        })
    }
}
