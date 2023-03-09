// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Utility for generating programmable transactions, either by specifying a command or for
//! migrating legacy transactions

use anyhow::Context;
use indexmap::{IndexMap, IndexSet};
use move_core_types::{ident_str, identifier::Identifier, language_storage::TypeTag};
use serde::Serialize;

use crate::{
    base_types::{ObjectID, ObjectRef, SuiAddress},
    messages::{
        Argument, CallArg, Command, ObjectArg, ProgrammableMoveCall, ProgrammableTransaction,
    },
    move_package::PACKAGE_MODULE_NAME,
    SUI_FRAMEWORK_OBJECT_ID,
};

#[derive(Default)]
pub struct ProgrammableTransactionBuilder {
    inputs: IndexSet<CallArg>,
    commands: Vec<Command>,
}

impl ProgrammableTransactionBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn finish(self) -> ProgrammableTransaction {
        let Self { inputs, commands } = self;
        let inputs = inputs.into_iter().collect();
        ProgrammableTransaction { inputs, commands }
    }

    pub fn pure<T: Serialize>(&mut self, value: T) -> anyhow::Result<Argument> {
        Ok(self
            .input(CallArg::Pure(
                bcs::to_bytes(&value).context("Searlizing pure argument.")?,
            ))
            .unwrap())
    }

    pub fn obj(&mut self, obj_arg: ObjectArg) -> Argument {
        self.input(CallArg::Object(obj_arg)).unwrap()
    }

    pub fn input(&mut self, call_arg: CallArg) -> anyhow::Result<Argument> {
        match call_arg {
            call_arg @ (CallArg::Pure(_) | CallArg::Object(_)) => {
                Ok(Argument::Input(self.inputs.insert_full(call_arg).0 as u16))
            }
            CallArg::ObjVec(objs) if objs.is_empty() => {
                anyhow::bail!(
                    "Empty ObjVec is not supported in programmable transactions \
                        without a type annotation"
                )
            }
            CallArg::ObjVec(objs) => Ok(self.make_obj_vec(objs)),
        }
    }

    pub fn make_obj_vec(&mut self, objs: impl IntoIterator<Item = ObjectArg>) -> Argument {
        let make_vec_args = objs.into_iter().map(|obj| self.obj(obj)).collect();
        self.command(Command::MakeMoveVec(None, make_vec_args))
    }

    pub fn command(&mut self, command: Command) -> Argument {
        let i = self.commands.len();
        self.commands.push(command);
        Argument::Result(i as u16)
    }

    /// Will fail to generate if given an empty ObjVec
    pub fn move_call(
        &mut self,
        package: ObjectID,
        module: Identifier,
        function: Identifier,
        type_arguments: Vec<TypeTag>,
        call_args: Vec<CallArg>,
    ) -> anyhow::Result<()> {
        let arguments = call_args
            .into_iter()
            .map(|a| self.input(a))
            .collect::<Result<_, _>>()?;
        self.command(Command::MoveCall(Box::new(ProgrammableMoveCall {
            package,
            module,
            function,
            type_arguments,
            arguments,
        })));
        Ok(())
    }

    pub fn programmable_move_call(
        &mut self,
        package: ObjectID,
        module: Identifier,
        function: Identifier,
        type_arguments: Vec<TypeTag>,
        arguments: Vec<Argument>,
    ) -> Argument {
        self.command(Command::MoveCall(Box::new(ProgrammableMoveCall {
            package,
            module,
            function,
            type_arguments,
            arguments,
        })))
    }

    pub fn publish_upgradeable(&mut self, modules: Vec<Vec<u8>>) -> Argument {
        self.command(Command::Publish(modules))
    }

    pub fn publish(&mut self, modules: Vec<Vec<u8>>) {
        let cap = self.publish_upgradeable(modules);
        self.commands
            .push(Command::MoveCall(Box::new(ProgrammableMoveCall {
                package: SUI_FRAMEWORK_OBJECT_ID,
                module: PACKAGE_MODULE_NAME.to_owned(),
                function: ident_str!("make_immutable").to_owned(),
                type_arguments: vec![],
                arguments: vec![cap],
            })));
    }

    pub fn transfer_arg(&mut self, recipient: SuiAddress, arg: Argument) {
        self.transfer_args(recipient, vec![arg])
    }

    pub fn transfer_args(&mut self, recipient: SuiAddress, args: Vec<Argument>) {
        let rec_arg = self.pure(recipient).unwrap();
        self.commands.push(Command::TransferObjects(args, rec_arg));
    }

    pub fn transfer_object(&mut self, recipient: SuiAddress, object_ref: ObjectRef) {
        let rec_arg = self.pure(recipient).unwrap();
        let obj_arg = self.obj(ObjectArg::ImmOrOwnedObject(object_ref));
        self.commands
            .push(Command::TransferObjects(vec![obj_arg], rec_arg));
    }

    pub fn transfer_sui(&mut self, recipient: SuiAddress, amount: Option<u64>) {
        let rec_arg = self.pure(recipient).unwrap();
        let coin_arg = if let Some(amount) = amount {
            let amt_arg = self.pure(amount).unwrap();
            self.command(Command::SplitCoin(Argument::GasCoin, amt_arg))
        } else {
            Argument::GasCoin
        };
        self.command(Command::TransferObjects(vec![coin_arg], rec_arg));
    }

    pub fn pay_all_sui(&mut self, recipient: SuiAddress) {
        let rec_arg = self.pure(recipient).unwrap();
        self.command(Command::TransferObjects(vec![Argument::GasCoin], rec_arg));
    }

    /// Will fail to generate if recipients and amounts do not have the same lengths
    pub fn pay_sui(
        &mut self,
        recipients: Vec<SuiAddress>,
        amounts: Vec<u64>,
    ) -> anyhow::Result<()> {
        self.pay_impl(recipients, amounts, Argument::GasCoin)
    }

    /// Will fail to generate if recipients and amounts do not have the same lengths.
    /// Or if coins is empty
    pub fn pay(
        &mut self,
        coins: Vec<ObjectRef>,
        recipients: Vec<SuiAddress>,
        amounts: Vec<u64>,
    ) -> anyhow::Result<()> {
        let mut coins = coins.into_iter();
        let Some(coin) = coins.next()
        else {
            anyhow::bail!("coins vector is empty");
        };
        let coin_arg = self.obj(ObjectArg::ImmOrOwnedObject(coin));
        let merge_args: Vec<_> = coins
            .map(|c| self.obj(ObjectArg::ImmOrOwnedObject(c)))
            .collect();
        if !merge_args.is_empty() {
            self.command(Command::MergeCoins(coin_arg, merge_args));
        }
        self.pay_impl(recipients, amounts, coin_arg)
    }

    fn pay_impl(
        &mut self,
        recipients: Vec<SuiAddress>,
        amounts: Vec<u64>,
        coin: Argument,
    ) -> anyhow::Result<()> {
        if recipients.len() != amounts.len() {
            anyhow::bail!(
                "Recipients and amounts mismatch. Got {} recipients but {} amounts",
                recipients.len(),
                amounts.len()
            )
        }

        // collect recipients in the case where they are non-unique in order
        // to minimize the number of transfers that must be performed
        let mut recipient_map = IndexMap::new();
        for (recipient, amount) in recipients.into_iter().zip(amounts) {
            recipient_map
                .entry(recipient)
                .or_insert_with(Vec::new)
                .push(amount);
        }
        for (recipient, amounts) in recipient_map {
            let rec_arg = self.pure(recipient).unwrap();
            let mut coins = vec![];
            for amount in amounts {
                let amt_arg = self.pure(amount).unwrap();
                coins.push(self.command(Command::SplitCoin(coin, amt_arg)));
            }
            self.command(Command::TransferObjects(coins, rec_arg));
        }
        Ok(())
    }
}
