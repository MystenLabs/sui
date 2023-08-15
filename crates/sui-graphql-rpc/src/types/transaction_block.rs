// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{address::Address, base64::Base64, sui_address::SuiAddress};
use async_graphql::*;

#[derive(SimpleObject, Clone, Eq, PartialEq)]
pub(crate) struct TransactionBlock {
    pub digest: String,
    pub sender: Option<Address>,
    pub bcs: Option<Base64>,
}

pub(crate) struct TransactionBlockConnection;

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
pub(crate) enum TransactionBlockKindInput {
    ConsensusCommitPrologue,
    Genesis,
    ChangeEpoch,
    Programmable,
}

#[derive(InputObject)]
pub(crate) struct TransactionBlockFilter {
    package: Option<SuiAddress>,
    module: Option<String>,
    function: Option<String>,

    kind: Option<TransactionBlockKindInput>,
    checkpoint: Option<u64>,

    sign_address: Option<SuiAddress>,
    sent_address: Option<SuiAddress>,
    recv_address: Option<SuiAddress>,
    paid_address: Option<SuiAddress>,

    input_object: Option<SuiAddress>,
    changed_object: Option<SuiAddress>,
}

#[allow(unreachable_code)]
#[allow(unused_variables)]
#[Object]
impl TransactionBlockConnection {
    async fn unimplemented(&self) -> bool {
        unimplemented!()
    }
}
