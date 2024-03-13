// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_core::authority::AuthorityStore;
use sui_replay::replay::LocalExec;
use sui_types::committee::EpochId;
use sui_types::digests::TransactionDigest;
use sui_types::transaction::Transaction;
use typed_store::rocks::DBMap;

pub struct LocalAuthority {
    pub store: AuthorityStore,
    pub transactions: DBMap<TransactionDigest, Transaction>,
    pub forked_epoch: EpochId,
    pub remote_url: String,
    pub local_exec: LocalExec,
}
