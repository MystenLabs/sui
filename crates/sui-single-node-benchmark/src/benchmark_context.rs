// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::command::Component;
use crate::mock_account::{batch_create_account_and_gas, Account};
use crate::single_node::SingleValidator;
use crate::tx_generator::{RootObjectCreateTxGenerator, TxGenerator};
use crate::workload::Workload;
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use std::collections::{BTreeMap, HashMap};
use std::ops::Deref;
use std::path::PathBuf;
use std::sync::Arc;
use sui_types::base_types::{ObjectID, ObjectRef, SuiAddress, TransactionDigest};
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI};
use sui_types::messages_grpc::HandleTransactionResponse;
use sui_types::mock_checkpoint_builder::ValidatorKeypairProvider;
use sui_types::transaction::{CertifiedTransaction, SignedTransaction, Transaction};
use tracing::info;

pub struct BenchmarkContext {
    validator: SingleValidator,
    user_accounts: BTreeMap<SuiAddress, Account>,
    admin_account: Account,
    benchmark_component: Component,
}

impl BenchmarkContext {
    pub(crate) async fn new(
        workload: Workload,
        benchmark_component: Component,
        checkpoint_size: usize,
    ) -> Self {
        // Increase by 2 so that we could generate one extra sample transaction before benchmarking.
        // as well as reserve 1 account for package publishing.
        let num_accounts = workload.num_accounts() + 2;
        let gas_object_num_per_account = workload.gas_object_num_per_account();
        let total = num_accounts * gas_object_num_per_account;

        info!(
            "Creating {} accounts and {} gas objects",
            num_accounts, total
        );
        let (mut user_accounts, genesis_gas_objects) =
            batch_create_account_and_gas(num_accounts, gas_object_num_per_account).await;
        assert_eq!(genesis_gas_objects.len() as u64, total);
        let (_, admin_account) = user_accounts.pop_last().unwrap();

        info!("Initializing validator");
        let validator =
            SingleValidator::new(&genesis_gas_objects, benchmark_component, checkpoint_size).await;

        Self {
            validator,
            user_accounts,
            admin_account,
            benchmark_component,
        }
    }

    pub(crate) fn validator(&self) -> SingleValidator {
        self.validator.clone()
    }

    pub(crate) async fn publish_package(&mut self) -> ObjectRef {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.extend(["move_package"]);
        let mut gas_objects = self.admin_account.gas_objects.deref().clone();
        let (package, updated_gas) = self
            .validator
            .publish_package(
                path,
                self.admin_account.sender,
                &self.admin_account.keypair,
                gas_objects[0],
            )
            .await;
        gas_objects[0] = updated_gas;
        self.admin_account.gas_objects = Arc::new(gas_objects);
        package
    }

    /// In order to benchmark transactions that can read dynamic fields, we must first create
    /// a root object with dynamic fields for each account address.
    pub(crate) async fn preparing_dynamic_fields(
        &mut self,
        move_package: ObjectID,
        num_dynamic_fields: u64,
    ) -> HashMap<SuiAddress, ObjectRef> {
        let mut root_objects = HashMap::new();

        if num_dynamic_fields == 0 {
            return root_objects;
        }

        info!("Preparing root object with dynamic fields");
        let root_object_create_transactions = self
            .generate_transactions(Arc::new(RootObjectCreateTxGenerator::new(
                move_package,
                num_dynamic_fields,
            )))
            .await;
        let results = self
            .execute_transactions_immediately(root_object_create_transactions)
            .await;
        let mut new_gas_objects = HashMap::new();
        for effects in results {
            let (owner, root_object) = effects
                .created()
                .into_iter()
                .filter_map(|(oref, owner)| {
                    owner
                        .get_address_owner_address()
                        .ok()
                        .map(|owner| (owner, oref))
                })
                .next()
                .unwrap();
            root_objects.insert(owner, root_object);
            let gas_object = effects.gas_object().0;
            new_gas_objects.insert(gas_object.0, gas_object);
        }
        self.refresh_gas_objects(new_gas_objects);
        info!("Finished preparing root object with dynamic fields");
        root_objects
    }

    pub(crate) async fn generate_transactions(
        &self,
        tx_generator: Arc<dyn TxGenerator>,
    ) -> Vec<Transaction> {
        info!(
            "{}: Creating {} transactions",
            tx_generator.name(),
            self.user_accounts.len()
        );
        let tasks: FuturesUnordered<_> = self
            .user_accounts
            .values()
            .map(|account| {
                let account = account.clone();
                let tx_generator = tx_generator.clone();
                tokio::spawn(async move { tx_generator.generate_tx(account) })
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

    async fn execute_transactions_immediately(
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

    pub(crate) async fn revert_transactions(
        &self,
        transactions: impl Iterator<Item = &TransactionDigest>,
    ) {
        let tasks: FuturesUnordered<_> = transactions
            .map(|digest| {
                let validator = self.validator();
                let digest = *digest;
                tokio::spawn(async move {
                    validator
                        .get_validator()
                        .db()
                        .revert_state_update(&digest)
                        .await
                        .unwrap()
                })
            })
            .collect();
        let results: Vec<_> = tasks.collect().await;
        results.into_iter().all(|r| r.is_ok());
    }

    fn refresh_gas_objects(&mut self, mut new_gas_objects: HashMap<ObjectID, ObjectRef>) {
        info!("Refreshing gas objects");
        for account in self.user_accounts.values_mut() {
            let refreshed_gas_objects: Vec<_> = account
                .gas_objects
                .iter()
                .map(|oref| {
                    if let Some(new_oref) = new_gas_objects.remove(&oref.0) {
                        new_oref
                    } else {
                        *oref
                    }
                })
                .collect();
            account.gas_objects = Arc::new(refreshed_gas_objects);
        }
    }
    pub(crate) async fn validator_sign_transactions(
        &self,
        transactions: Vec<Transaction>,
    ) -> Vec<HandleTransactionResponse> {
        info!(
            "Started signing {} transactions. You can now attach a profiler",
            transactions.len(),
        );
        let tasks: FuturesUnordered<_> = transactions
            .into_iter()
            .map(|tx| {
                let validator = self.validator();
                tokio::spawn(async move { validator.sign_transaction(tx).await })
            })
            .collect();
        let results: Vec<_> = tasks.collect().await;
        results.into_iter().map(|r| r.unwrap()).collect()
    }
}
