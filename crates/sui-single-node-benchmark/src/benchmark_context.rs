// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::single_node::SingleValidator;
use crate::TxGenerator;
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use std::collections::HashMap;
use std::path::PathBuf;
use sui_types::base_types::{ObjectID, ObjectRef, SuiAddress, SUI_ADDRESS_LENGTH};
use sui_types::crypto::{get_account_key_pair, AccountKeyPair};
use sui_types::effects::TransactionEffects;
use sui_types::executable_transaction::VerifiedExecutableTransaction;
use sui_types::object::Object;
use tracing::info;

pub struct BenchmarkContext {
    validator: SingleValidator,
    accounts: Vec<(SuiAddress, AccountKeyPair)>,
    gas_object_refs: Vec<Vec<ObjectRef>>,
    admin_account: (SuiAddress, AccountKeyPair),
    admin_gas: ObjectID,
}

impl BenchmarkContext {
    pub(crate) async fn new(num_accounts: u64, gas_object_num_per_account: u64) -> Self {
        // Increase by 1 so that we could generate one extra sample transaction before benchmarking.
        let num_accounts = num_accounts + 1;
        info!(
            "Creating {} gas objects",
            num_accounts * gas_object_num_per_account
        );
        let accounts = (0..num_accounts)
            .map(|_| get_account_key_pair())
            .collect::<Vec<_>>();
        let mut idx: u64 = 0;
        let mut gas_objects = vec![];
        let mut gas_object_refs = vec![];
        for (sender, _) in &accounts {
            let mut gas_object_refs_for_account = vec![];
            for _ in 0..gas_object_num_per_account {
                let object = Self::new_gas_object(idx, *sender);
                idx += 1;
                gas_object_refs_for_account.push(object.compute_object_reference());
                gas_objects.push(object);
            }
            gas_object_refs.push(gas_object_refs_for_account);
        }
        // Admin account and gas can be used to publish package and other admin operations.
        let admin_account = get_account_key_pair();
        let admin_gas_object = Self::new_gas_object(idx, admin_account.0);
        let admin_gas = admin_gas_object.id();
        gas_objects.push(admin_gas_object);

        info!("Initializing validator");
        let validator = SingleValidator::new(&gas_objects).await;

        Self {
            validator,
            accounts,
            gas_object_refs,
            admin_account,
            admin_gas,
        }
    }

    pub fn validator(&self) -> SingleValidator {
        self.validator.clone()
    }

    fn new_gas_object(idx: u64, owner: SuiAddress) -> Object {
        // Predictable and cheaper way of generating object IDs for benchmarking.
        let mut id_bytes = [0u8; SUI_ADDRESS_LENGTH];
        let idx_bytes = idx.to_le_bytes();
        id_bytes[0] = 255;
        id_bytes[1..idx_bytes.len() + 1].copy_from_slice(&idx_bytes);
        let object_id = ObjectID::from_bytes(id_bytes).unwrap();
        Object::with_id_owner_for_testing(object_id, owner)
    }

    pub(crate) async fn publish_package(&self) -> ObjectRef {
        let gas = self.validator.get_latest_object_ref(&self.admin_gas).await;
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.extend(["move_package"]);
        self.validator
            .publish_package(path, self.admin_account.0, &self.admin_account.1, gas)
            .await
    }

    pub(crate) async fn generate_transactions(
        &self,
        tx_generator: impl TxGenerator,
    ) -> Vec<VerifiedExecutableTransaction> {
        info!(
            "{}: Creating {} transactions",
            tx_generator.name(),
            self.accounts.len()
        );
        self.accounts
            .iter()
            .zip(self.gas_object_refs.iter())
            .map(|((sender, keypair), gas)| tx_generator.generate_tx(*sender, keypair, gas))
            .collect()
    }

    pub(crate) async fn execute_transactions(
        &self,
        transactions: Vec<VerifiedExecutableTransaction>,
    ) -> Vec<TransactionEffects> {
        info!("Started executing {} transactions", transactions.len());
        let tasks: FuturesUnordered<_> = transactions
            .into_iter()
            .map(|tx| {
                let validator = self.validator();
                tokio::spawn(async move { validator.execute_tx_immediately(tx).await })
            })
            .collect();
        let results: Vec<_> = tasks.collect().await;
        results.into_iter().map(|r| r.unwrap()).collect()
    }

    pub(crate) fn refresh_gas_objects(
        &mut self,
        mut new_gas_objects: HashMap<ObjectID, ObjectRef>,
    ) {
        info!("Refreshing gas objects");
        for gas_objects in self.gas_object_refs.iter_mut() {
            for oref in gas_objects.iter_mut() {
                if let Some(new_oref) = new_gas_objects.remove(&oref.0) {
                    *oref = new_oref;
                }
            }
        }
    }
}
