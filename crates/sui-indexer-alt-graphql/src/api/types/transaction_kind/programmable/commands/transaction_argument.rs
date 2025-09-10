// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;
use sui_rpc::proto::sui::rpc::v2beta2 as proto;
use sui_types::transaction::Argument as NativeArgument;

/// An argument to a programmable transaction command.
#[derive(Union, Clone)]
pub enum TransactionArgument {
    GasCoin(GasCoin),
    Input(Input),
    Result(TxResult),
}

/// Access to the gas inputs, after they have been smashed into one coin. The gas coin can only be used by reference, except for with `TransferObjectsTransaction` that can accept it by value.
#[derive(SimpleObject, Clone)]
pub struct GasCoin {
    /// Placeholder field (gas coin has no additional data)
    #[graphql(name = "_")]
    pub dummy: Option<bool>,
}

// One of the input objects or primitive values to the programmable transaction.
#[derive(SimpleObject, Clone)]
pub struct Input {
    /// The index of the input.
    pub ix: Option<u16>,
}

/// The result of another command.
#[derive(SimpleObject, Clone)]
pub struct TxResult {
    /// The index of the command that produced this result.
    pub cmd: Option<u16>,
    /// For nested results, the index within the result.
    pub ix: Option<u16>,
}

impl From<NativeArgument> for TransactionArgument {
    fn from(argument: NativeArgument) -> Self {
        match argument {
            NativeArgument::GasCoin => TransactionArgument::GasCoin(GasCoin { dummy: None }),
            NativeArgument::Input(ix) => TransactionArgument::Input(Input { ix: Some(ix) }),
            NativeArgument::Result(cmd) => TransactionArgument::Result(TxResult {
                cmd: Some(cmd),
                ix: None,
            }),
            NativeArgument::NestedResult(cmd, ix) => TransactionArgument::Result(TxResult {
                cmd: Some(cmd),
                ix: Some(ix),
            }),
        }
    }
}

impl TryFrom<proto::Argument> for TransactionArgument {
    type Error = ();

    fn try_from(arg: proto::Argument) -> Result<Self, Self::Error> {
        use proto::argument::ArgumentKind;

        match arg.kind() {
            ArgumentKind::Gas => Ok(TransactionArgument::GasCoin(GasCoin { dummy: None })),
            ArgumentKind::Input => Ok(TransactionArgument::Input(Input {
                ix: Some(arg.input.unwrap_or_default() as u16),
            })),
            ArgumentKind::Result => Ok(TransactionArgument::Result(TxResult {
                cmd: Some(arg.result.unwrap_or_default() as u16),
                ix: arg.subresult.map(|subresult| subresult as u16),
            })),
            _ => Err(()),
        }
    }
}
