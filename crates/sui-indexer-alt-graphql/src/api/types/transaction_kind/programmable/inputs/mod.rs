// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod object;
pub mod pure;

use async_graphql::*;

use crate::{api::scalars::base64::Base64, scope::Scope};
pub use object::{OwnedOrImmutable, Receiving, SharedInput};
pub use pure::Pure;

/// Input argument to a Programmable Transaction Block (PTB) command.
#[derive(Union)]
pub enum TransactionInput {
    Pure(Pure),
    OwnedOrImmutable(OwnedOrImmutable),
    SharedInput(SharedInput),
    Receiving(Receiving),
    // TODO: Add BalanceWithdraw variant
}

impl TransactionInput {
    pub fn from(input: sui_types::transaction::CallArg, scope: Scope) -> Self {
        use sui_types::transaction::{CallArg, ObjectArg};

        match input {
            CallArg::Pure(bytes) => Self::Pure(Pure {
                bytes: Some(Base64::from(bytes)),
            }),
            CallArg::Object(obj_arg) => match obj_arg {
                ObjectArg::ImmOrOwnedObject((object_id, version, digest)) => {
                    Self::OwnedOrImmutable(OwnedOrImmutable::from_object_ref(
                        object_id, version, digest, scope,
                    ))
                }
                ObjectArg::SharedObject {
                    id,
                    initial_shared_version,
                    mutable,
                } => Self::SharedInput(SharedInput::from_shared_object(
                    id,
                    initial_shared_version,
                    mutable,
                )),
                ObjectArg::Receiving((object_id, version, digest)) => Self::Receiving(
                    Receiving::from_object_ref(object_id, version, digest, scope),
                ),
            },
            // TODO: Handle BalanceWithdraw
            CallArg::BalanceWithdraw(_) => Self::Pure(Pure {
                bytes: Some(Base64::from(
                    b"TODO: BalanceWithdraw not supported".to_vec(),
                )),
            }),
        }
    }
}
