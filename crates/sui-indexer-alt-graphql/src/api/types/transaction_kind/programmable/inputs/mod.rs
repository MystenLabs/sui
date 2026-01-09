// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::Union;

use sui_types::transaction::CallArg;
use sui_types::transaction::ObjectArg;

use crate::api::scalars::base64::Base64;
use crate::scope::Scope;

pub use object::OwnedOrImmutable;
pub use object::Receiving;
pub use object::SharedInput;
pub use pure::Pure;

pub mod object;
pub mod pure;

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
    pub fn from(input: CallArg, scope: Scope) -> Self {
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
                    mutability,
                } => Self::SharedInput(SharedInput::from_shared_object(
                    id,
                    initial_shared_version,
                    // TODO: extend schema to expose the full mutability enum
                    mutability.is_exclusive(),
                )),
                ObjectArg::Receiving((object_id, version, digest)) => Self::Receiving(
                    Receiving::from_object_ref(object_id, version, digest, scope),
                ),
            },
            // TODO: Handle FundsWithdrawal
            CallArg::FundsWithdrawal(_) => Self::Pure(Pure {
                bytes: Some(Base64::from(
                    b"TODO: FundsWithdrawal not supported".to_vec(),
                )),
            }),
        }
    }
}
