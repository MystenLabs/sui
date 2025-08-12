// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod pure;

use async_graphql::*;

use crate::{api::scalars::base64::Base64, scope::Scope};
pub use pure::Pure;

/// Input argument to a Programmable Transaction Block (PTB) command.
#[derive(Union, Clone)]
pub enum TransactionInput {
    Pure(Pure),
}

impl TransactionInput {
    pub fn from(input: sui_types::transaction::CallArg, _scope: Scope) -> Self {
        use sui_types::transaction::CallArg;

        match input {
            CallArg::Pure(bytes) => Self::Pure(Pure {
                bytes: Some(Base64::from(bytes)),
            }),
            // TODO: Handle other input types
            _ => Self::Pure(Pure {
                bytes: Some(Base64::from(b"TODO: Unsupported input type".to_vec())),
            }),
        }
    }
}
