// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_types::base_types::{ObjectRef, SuiAddress};
use sui_types::crypto::AccountKeyPair;
use sui_types::executable_transaction::VerifiedExecutableTransaction;

pub(crate) mod benchmark_context;
pub mod command;
pub mod execution;
pub(crate) mod move_tx_generator;
pub(crate) mod non_move_tx_generator;
pub(crate) mod root_object_create_tx_generator;
pub(crate) mod single_node;

pub(crate) trait TxGenerator {
    /// Given a sender address, a keypair for that address, and a list of gas objects owned
    /// by this address, generate a single executable transaction.
    fn generate_tx(
        &self,
        sender: SuiAddress,
        keypair: &AccountKeyPair,
        gas_objects: &[ObjectRef],
    ) -> VerifiedExecutableTransaction;

    fn name(&self) -> &'static str;
}
