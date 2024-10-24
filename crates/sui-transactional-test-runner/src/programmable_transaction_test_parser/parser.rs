// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{borrow::BorrowMut, marker::PhantomData, str::FromStr};

use move_core_types::parsing::{
    parser::{Parser, Token},
    types::{ParsedType, TypeToken},
};
use move_core_types::{account_address::AccountAddress, identifier::Identifier};
use sui_types::{
    base_types::ObjectID,
    transaction::{Argument, Command, ProgrammableMoveCall},
    type_input::TypeInput,
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
> {
    inner: P,
    _a: PhantomData<&'a ()>,
    _i: PhantomData<I>,
}

#[derive(Debug, Clone)]
pub struct ParsedMoveCall {
    pub package: Identifier,
    pub module: Identifier,
    pub function: Identifier,
    pub type_arguments: Vec<ParsedType>,
    pub arguments: Vec<Argument>,
}

#[derive(Debug, Clone)]
pub enum ParsedCommand {
    MoveCall(Box<ParsedMoveCall>),
    TransferObjects(Vec<Argument>, Argument),
    SplitCoins(Argument, Vec<Argument>),
    MergeCoins(Argument, Vec<Argument>),
    MakeMoveVec(Option<ParsedType>, Vec<Argument>),
    Publish(String, Vec<String>),
    Upgrade(String, Vec<String>, String, Argument),
}

impl<'a, I: Iterator<Item = (CommandToken, &'a str)>>
    CommandParser<'a, I, Parser<'a, CommandToken, I>>
{
    pub fn new<T: IntoIterator<Item = (CommandToken, &'a str), IntoIter = I>>(v: T) -> Self {
        Self::from_parser(Parser::new(v))
    }
}

impl<'a, I, P> CommandParser<'a, I, P>
where
    I: Iterator<Item = (CommandToken, &'a str)>,
    P: BorrowMut<Parser<'a, CommandToken, I>>,
{
    pub fn from_parser(inner: P) -> Self {
        Self {
            inner,
            _a: PhantomData,
            _i: PhantomData,
        }
    }

    pub fn parse_commands(&mut self) -> Result<Vec<ParsedCommand>> {
        let commands = self.inner().parse_list(
            |p| CommandParser::from_parser(p).parse_command_start(),
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
        self.inner().advance(CommandToken::CommandStart)?;
        let idx = if let Some(CommandToken::Number) = self.inner().peek_tok() {
            let num = self.inner().advance(CommandToken::Number)?;
            let idx = usize::from_str(num).context("Invalid command index annotation")?;
            self.inner().advance(CommandToken::Colon)?;
            Some(idx)
        } else {
            None
        };
        let cmd = self.parse_command()?;
        Ok((idx, cmd))
    }

    pub fn parse_command(&mut self) -> Result<ParsedCommand> {
        use super::token::CommandToken as Tok;
        Ok(match self.inner().advance_any()? {
            (Tok::Ident, TRANSFER_OBJECTS) => {
                self.inner().advance(Tok::LParen)?;
                let args = self.parse_command_args(Tok::LBracket, Tok::RBracket)?;
                self.inner().advance(Tok::Comma)?;
                let arg = self.parse_command_arg()?;
                self.maybe_trailing_comma()?;
                self.inner().advance(Tok::RParen)?;
                ParsedCommand::TransferObjects(args, arg)
            }
            (Tok::Ident, SPLIT_COINS) => {
                self.inner().advance(Tok::LParen)?;
                let coin = self.parse_command_arg()?;
                self.inner().advance(Tok::Comma)?;
                let amts = self.parse_command_args(Tok::LBracket, Tok::RBracket)?;
                self.maybe_trailing_comma()?;
                self.inner().advance(Tok::RParen)?;
                ParsedCommand::SplitCoins(coin, amts)
            }
            (Tok::Ident, MERGE_COINS) => {
                self.inner().advance(Tok::LParen)?;
                let target = self.parse_command_arg()?;
                self.inner().advance(Tok::Comma)?;
                let coins = self.parse_command_args(Tok::LBracket, Tok::RBracket)?;
                self.maybe_trailing_comma()?;
                self.inner().advance(Tok::RParen)?;
                ParsedCommand::MergeCoins(target, coins)
            }
            (Tok::Ident, MAKE_MOVE_VEC) => {
                let type_opt = self.parse_type_arg_opt()?;
                self.inner().advance(Tok::LParen)?;
                let args = self.parse_command_args(Tok::LBracket, Tok::RBracket)?;
                self.maybe_trailing_comma()?;
                self.inner().advance(Tok::RParen)?;
                ParsedCommand::MakeMoveVec(type_opt, args)
            }
            (Tok::Ident, PUBLISH) => {
                self.inner().advance(Tok::LParen)?;
                let staged_package = self.inner().advance(Tok::Ident)?;
                self.inner().advance(Tok::Comma)?;
                self.inner().advance(Tok::LBracket)?;
                let dependencies = self.inner().parse_list(
                    |p| Ok(p.advance(Tok::Ident)?.to_owned()),
                    CommandToken::Comma,
                    Tok::RBracket,
                    /* allow_trailing_delim */ true,
                )?;
                self.inner().advance(Tok::RBracket)?;
                self.maybe_trailing_comma()?;
                self.inner().advance(Tok::RParen)?;
                ParsedCommand::Publish(staged_package.to_owned(), dependencies)
            }
            (Tok::Ident, UPGRADE) => {
                self.inner().advance(Tok::LParen)?;
                let staged_package = self.inner().advance(Tok::Ident)?;
                self.inner().advance(Tok::Comma)?;
                self.inner().advance(Tok::LBracket)?;
                let dependencies = self.inner().parse_list(
                    |p| Ok(p.advance(Tok::Ident)?.to_owned()),
                    CommandToken::Comma,
                    Tok::RBracket,
                    /* allow_trailing_delim */ true,
                )?;
                self.inner().advance(Tok::RBracket)?;
                self.inner().advance(Tok::Comma)?;
                let upgraded_package = self.inner().advance(Tok::Ident)?;
                self.inner().advance(Tok::Comma)?;
                let upgrade_ticket = self.parse_command_arg()?;
                self.maybe_trailing_comma()?;
                self.inner().advance(Tok::RParen)?;
                ParsedCommand::Upgrade(
                    staged_package.to_owned(),
                    dependencies,
                    upgraded_package.to_owned(),
                    upgrade_ticket,
                )
            }
            (Tok::Ident, contents) => {
                let package = Identifier::new(contents)?;
                self.inner().advance(Tok::ColonColon)?;
                let module = Identifier::new(self.inner().advance(Tok::Ident)?)?;
                self.inner().advance(Tok::ColonColon)?;
                let function = Identifier::new(self.inner().advance(Tok::Ident)?)?;
                let type_arguments = self.parse_type_args_opt()?.unwrap_or_default();
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
        if let Some(CommandToken::Comma) = self.inner().peek_tok() {
            self.inner().advance(CommandToken::Comma)?;
        }
        Ok(())
    }

    pub fn parse_command_args(
        &mut self,
        start: CommandToken,
        end: CommandToken,
    ) -> Result<Vec<Argument>> {
        self.inner().advance(start)?;
        let args = self.inner().parse_list(
            |p| CommandParser::from_parser(p).parse_command_arg(),
            CommandToken::Comma,
            end,
            /* allow_trailing_delim */ true,
        )?;
        self.inner().advance(end)?;
        Ok(args)
    }

    pub fn parse_command_arg(&mut self) -> Result<Argument> {
        use super::token::CommandToken as Tok;
        Ok(match self.inner().advance_any()? {
            (Tok::Ident, GAS_COIN) => Argument::GasCoin,
            (Tok::Ident, INPUT) => {
                self.inner().advance(Tok::LParen)?;
                let num = self.parse_u16()?;
                self.maybe_trailing_comma()?;
                self.inner().advance(Tok::RParen)?;
                Argument::Input(num)
            }
            (Tok::Ident, RESULT) => {
                self.inner().advance(Tok::LParen)?;
                let num = self.parse_u16()?;
                self.maybe_trailing_comma()?;
                self.inner().advance(Tok::RParen)?;
                Argument::Result(num)
            }
            (Tok::Ident, NESTED_RESULT) => {
                self.inner().advance(Tok::LParen)?;
                let i = self.parse_u16()?;
                self.inner().advance(Tok::Comma)?;
                let j = self.parse_u16()?;
                self.maybe_trailing_comma()?;
                self.inner().advance(Tok::RParen)?;
                Argument::NestedResult(i, j)
            }
            (tok, _) => bail!("unexpected token {}, expected argument identifier", tok),
        })
    }

    pub fn parse_u16(&mut self) -> Result<u16> {
        let contents = self.inner().advance(CommandToken::Number)?;
        u16::from_str(contents).context("Expected u16 for Argument")
    }

    pub fn parse_type_arg_opt(&mut self) -> Result<Option<ParsedType>> {
        match self.parse_type_args_opt()? {
            None => Ok(None),
            Some(v) if v.len() != 1 => bail!(
                "unexpected multiple type arguments. Expected 1 type argument but got {}",
                v.len()
            ),
            Some(mut v) => Ok(Some(v.pop().unwrap())),
        }
    }

    pub fn parse_type_args_opt(&mut self) -> Result<Option<Vec<ParsedType>>> {
        if !matches!(self.inner().peek_tok(), Some(CommandToken::TypeArgString)) {
            return Ok(None);
        }
        let contents = self.inner().advance(CommandToken::TypeArgString)?;
        let type_tokens: Vec<_> = TypeToken::tokenize(contents)?
            .into_iter()
            .filter(|(tok, _)| !tok.is_whitespace())
            .collect();
        let mut parser = Parser::new(type_tokens);
        parser.advance(TypeToken::Lt)?;
        let res = parser.parse_list(|p| p.parse_type(), TypeToken::Comma, TypeToken::Gt, true)?;
        parser.advance(TypeToken::Gt)?;
        if let Ok((_, contents)) = parser.advance_any() {
            bail!("Expected end of token stream. Got: {}", contents)
        }
        Ok(Some(res))
    }

    pub fn inner(&mut self) -> &mut Parser<'a, CommandToken, I> {
        self.inner.borrow_mut()
    }
}

impl ParsedCommand {
    pub fn parse_vec(s: &str) -> Result<Vec<Self>> {
        let tokens: Vec<_> = CommandToken::tokenize(s)?
            .into_iter()
            .filter(|(tok, _)| !tok.is_whitespace())
            .collect();
        let mut parser = CommandParser::new(tokens);
        let res = parser.parse_commands()?;
        if let Ok((_, contents)) = parser.inner().advance_any() {
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
            ParsedCommand::MakeMoveVec(ty_opt, args) => Command::make_move_vec(
                ty_opt
                    .map(|t| t.into_type_tag(address_mapping))
                    .transpose()?,
                args,
            ),
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
        let type_arguments = type_arguments
            .into_iter()
            .map(|t| t.into_type_tag(address_mapping).map(TypeInput::from))
            .collect::<Result<_>>()?;
        Ok(ProgrammableMoveCall {
            package: package.into(),
            module: module.to_string(),
            function: function.to_string(),
            type_arguments,
            arguments,
        })
    }
}
