// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::command::Component;
use crate::single_node::SingleValidator;
use crate::tx_generator::TxGenerator;
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use sui_types::base_types::{ObjectID, ObjectRef, SuiAddress, SUI_ADDRESS_LENGTH};
use sui_types::crypto::{get_account_key_pair, AccountKeyPair};
use sui_types::effects::TransactionEffects;
use sui_types::object::Object;
use sui_types::transaction::{CertifiedTransaction, SignedTransaction, Transaction};
use tracing::info;

type Account = (SuiAddress, Arc<AccountKeyPair>);

pub struct BenchmarkContext {
    validator: SingleValidator,
    accounts: Vec<Account>,
    gas_object_refs: Vec<Arc<Vec<ObjectRef>>>,
    admin_account: Account,
    admin_gas: ObjectID,
    benchmark_component: Component,
}

impl BenchmarkContext {
    pub(crate) async fn new(
        num_accounts: u64,
        gas_object_num_per_account: u64,
        benchmark_component: Component,
    ) -> Self {
        // Increase by 1 so that we could generate one extra sample transaction before benchmarking.
        let num_accounts = num_accounts + 1;
        let total = num_accounts * gas_object_num_per_account;

        info!(
            "Creating {} accounts and {} gas objects",
            num_accounts, total
        );
        let results =
            Self::batch_create_account_and_gas(num_accounts, gas_object_num_per_account).await;
        let mut accounts = vec![];
        let mut gas_object_refs = vec![];
        let mut genesis_gas_objects = vec![];
        results
            .into_iter()
            .for_each(|((sender, keypair), gas_objects)| {
                accounts.push((sender, keypair));
                gas_object_refs.push(Arc::new(
                    gas_objects
                        .iter()
                        .map(|o| o.compute_object_reference())
                        .collect(),
                ));
                genesis_gas_objects.extend(gas_objects);
            });
        assert_eq!(genesis_gas_objects.len() as u64, total);

        // Admin account and gas can be used to publish package and other admin operations.
        let (admin_addr, admin_keypair) = get_account_key_pair();
        let admin_account = (admin_addr, Arc::new(admin_keypair));
        let admin_gas_object = Self::new_gas_object(total, admin_addr);
        let admin_gas = admin_gas_object.id();
        genesis_gas_objects.push(admin_gas_object);

        info!("Initializing validator");
        let validator = SingleValidator::new(&genesis_gas_objects).await;

        Self {
            validator,
            accounts,
            gas_object_refs,
            admin_account,
            admin_gas,
            benchmark_component,
        }
    }

    pub fn validator(&self) -> SingleValidator {
        self.validator.clone()
    }

    async fn batch_create_account_and_gas(
        num_accounts: u64,
        gas_object_num_per_account: u64,
    ) -> Vec<(Account, Vec<Object>)> {
        let tasks: FuturesUnordered<_> = (0..num_accounts)
            .map(|idx| {
                let starting_id = idx * gas_object_num_per_account;
                tokio::spawn(async move {
                    let (sender, keypair) = get_account_key_pair();
                    let objects = (0..gas_object_num_per_account)
                        .map(|i| Self::new_gas_object(starting_id + i, sender))
                        .collect::<Vec<_>>();
                    ((sender, Arc::new(keypair)), objects)
                })
            })
            .collect();
        tasks
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .map(|r| r.unwrap())
            .collect()
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
        tx_generator: Arc<dyn TxGenerator>,
    ) -> Vec<Transaction> {
        info!(
            "{}: Creating {} transactions",
            tx_generator.name(),
            self.accounts.len()
        );
        let tasks: FuturesUnordered<_> = self
            .accounts
            .iter()
            .zip(self.gas_object_refs.iter())
            .map(|((sender, keypair), gas)| {
                let sender = *sender;
                let keypair = keypair.clone();
                let gas = gas.clone();
                let tx_generator = tx_generator.clone();
                tokio::spawn(async move { tx_generator.generate_tx(sender, keypair, gas) })
            })
            .collect();
        let results: Vec<_> = tasks.collect().await;
        results.into_iter().map(|r| r.unwrap()).collect()
    }

    pub(crate) async fn certify_transactions(
        &self,
        transactions: Vec<Transaction>,
    ) -> Vec<CertifiedTransaction> {
        info!("Creating transaction certificates");
        let tasks: FuturesUnordered<_> = transactions
            .into_iter()
            .map(|tx| {
                let validator = self.validator();
                tokio::spawn(async move {
                    let committee = validator.get_committee();
                    let validator = validator.get_validator();
                    let sig = SignedTransaction::sign(0, &tx, &*validator.secret, validator.name);
                    CertifiedTransaction::new(tx.into_data(), vec![sig], committee).unwrap()
                })
            })
            .collect();
        let results: Vec<_> = tasks.collect().await;
        results.into_iter().map(|r| r.unwrap()).collect()
    }

    pub(crate) async fn execute_transactions(
        &self,
        transactions: Vec<CertifiedTransaction>,
    ) -> Vec<TransactionEffects> {
        info!(
            "Started executing {} transactions. You can now attach a profiler",
            transactions.len()
        );
        let tasks: FuturesUnordered<_> = transactions
            .into_iter()
            .map(|tx| {
                let validator = self.validator();
                let component = self.benchmark_component;
                tokio::spawn(async move { validator.execute_transaction(tx, component).await })
            })
            .collect();
        let results: Vec<_> = tasks.collect().await;
        results.into_iter().map(|r| r.unwrap()).collect()
    }

    pub(crate) async fn execute_transactions_immediately(
        &self,
        transactions: Vec<Transaction>,
    ) -> Vec<TransactionEffects> {
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
            let refreshed_gas_objects: Vec<_> = gas_objects
                .iter()
                .map(|oref| {
                    if let Some(new_oref) = new_gas_objects.remove(&oref.0) {
                        new_oref
                    } else {
                        *oref
                    }
                })
                .collect();
            *gas_objects = Arc::new(refreshed_gas_objects);
        }
    }
}
