// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::sui_client_config::{SuiClientConfig, SuiEnv};
use crate::SuiClient;
use anyhow::{anyhow, ensure};
use futures::future;
use shared_crypto::intent::Intent;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use sui_config::{Config, PersistedConfig};
use sui_json_rpc_types::{
    SuiObjectData, SuiObjectDataFilter, SuiObjectDataOptions, SuiObjectResponse,
    SuiObjectResponseQuery, SuiTransactionBlockResponse, SuiTransactionBlockResponseOptions,
};
use sui_keys::key_identity::KeyIdentity;
use sui_keys::keystore::{AccountKeystore, Keystore};
use sui_types::base_types::{FullObjectRef, ObjectID, ObjectRef, SuiAddress};
use sui_types::crypto::{Signature, SuiKeyPair};

use sui_types::gas_coin::GasCoin;
use sui_types::transaction::{Transaction, TransactionData, TransactionDataAPI};
use tokio::sync::RwLock;

pub struct WalletContext {
    pub config: PersistedConfig<SuiClientConfig>,
    request_timeout: Option<std::time::Duration>,
    client: Arc<RwLock<Option<SuiClient>>>,
    max_concurrent_requests: Option<u64>,
    env_override: Option<String>,
}

impl WalletContext {
    pub fn new(config_path: &Path) -> Result<Self, anyhow::Error> {
        let config: SuiClientConfig = PersistedConfig::read(config_path).map_err(|err| {
            anyhow!(
                "Cannot open wallet config file at {:?}. Err: {err}",
                config_path
            )
        })?;

        let config = config.persisted(config_path);
        let context = Self {
            config,
            request_timeout: None,
            client: Default::default(),
            max_concurrent_requests: None,
            env_override: None,
        };
        Ok(context)
    }

    pub fn new_for_tests(
        keystore: Keystore,
        external: Option<Keystore>,
        path: Option<PathBuf>,
    ) -> Self {
        let mut config = SuiClientConfig::new(keystore)
            .persisted(&path.unwrap_or(PathBuf::from("test_config.yaml")));
        config.external_keys = external;
        Self {
            config,
            request_timeout: None,
            client: Arc::new(Default::default()),
            max_concurrent_requests: None,
            env_override: None,
        }
    }

    pub fn with_request_timeout(mut self, request_timeout: std::time::Duration) -> Self {
        self.request_timeout = Some(request_timeout);
        self
    }

    pub fn with_max_concurrent_requests(mut self, max_concurrent_requests: u64) -> Self {
        self.max_concurrent_requests = Some(max_concurrent_requests);
        self
    }

    pub fn with_env_override(mut self, env_override: String) -> Self {
        self.env_override = Some(env_override);
        self
    }

    pub fn get_addresses(&self) -> Vec<SuiAddress> {
        self.config.keystore.addresses()
    }

    pub fn get_env_override(&self) -> Option<String> {
        self.env_override.clone()
    }

    pub fn get_identity_address(
        &mut self,
        input: Option<KeyIdentity>,
    ) -> Result<SuiAddress, anyhow::Error> {
        if let Some(key_identity) = input {
            if let Ok(address) = self.config.keystore.get_by_identity(&key_identity) {
                return Ok(address);
            }
            if let Some(address) = self
                .config
                .external_keys
                .as_ref()
                .and_then(|external_keys| external_keys.get_by_identity(&key_identity).ok())
            {
                return Ok(address);
            }

            Err(anyhow!(
                "No address found for the provided key identity: {key_identity}"
            ))
        } else {
            self.active_address()
        }
    }

    pub async fn get_client(&self) -> Result<SuiClient, anyhow::Error> {
        let read = self.client.read().await;

        Ok(if let Some(client) = read.as_ref() {
            client.clone()
        } else {
            drop(read);
            let client = self
                .get_active_env()?
                .create_rpc_client(self.request_timeout, self.max_concurrent_requests)
                .await?;
            self.client.write().await.insert(client).clone()
        })
    }

    pub fn get_active_env(&self) -> Result<&SuiEnv, anyhow::Error> {
        if self.env_override.is_some() {
            self.config.get_env(&self.env_override).ok_or_else(|| {
                anyhow!(
                    "Environment configuration not found for env [{}]",
                    self.env_override.as_deref().unwrap_or("None")
                )
            })
        } else {
            self.config.get_active_env()
        }
    }

    // TODO: Ger rid of mut
    pub fn active_address(&mut self) -> Result<SuiAddress, anyhow::Error> {
        if self.config.keystore.entries().is_empty() {
            return Err(anyhow!(
                "No managed addresses. Create new address with `new-address` command."
            ));
        }

        // Ok to unwrap because we checked that config addresses not empty
        // Set it if not exists
        self.config.active_address = Some(
            self.config
                .active_address
                .unwrap_or(*self.config.keystore.addresses().first().unwrap()),
        );

        Ok(self.config.active_address.unwrap())
    }

    /// Get the latest object reference given a object id
    pub async fn get_object_ref(&self, object_id: ObjectID) -> Result<ObjectRef, anyhow::Error> {
        let client = self.get_client().await?;
        Ok(client
            .read_api()
            .get_object_with_options(object_id, SuiObjectDataOptions::new())
            .await?
            .into_object()?
            .object_ref())
    }

    /// Get the latest full object reference given a object id
    pub async fn get_full_object_ref(
        &self,
        object_id: ObjectID,
    ) -> Result<FullObjectRef, anyhow::Error> {
        let client = self.get_client().await?;
        let object = client
            .read_api()
            .get_object_with_options(object_id, SuiObjectDataOptions::new().with_owner())
            .await?
            .into_object()?;
        let object_ref = object.object_ref();
        let owner = object
            .owner
            .expect("Owner should be present if `with_owner` is set");
        Ok(FullObjectRef::from_object_ref_and_owner(object_ref, &owner))
    }

    /// Get all the gas objects (and conveniently, gas amounts) for the address
    pub async fn gas_objects(
        &self,
        address: SuiAddress,
    ) -> Result<Vec<(u64, SuiObjectData)>, anyhow::Error> {
        let client = self.get_client().await?;

        let mut objects: Vec<SuiObjectResponse> = Vec::new();
        let mut cursor = None;
        loop {
            let response = client
                .read_api()
                .get_owned_objects(
                    address,
                    Some(SuiObjectResponseQuery::new(
                        Some(SuiObjectDataFilter::StructType(GasCoin::type_())),
                        Some(SuiObjectDataOptions::full_content()),
                    )),
                    cursor,
                    None,
                )
                .await?;

            objects.extend(response.data);

            if response.has_next_page {
                cursor = response.next_cursor;
            } else {
                break;
            }
        }

        // TODO: We should ideally fetch the objects from local cache
        let mut values_objects = Vec::new();

        for object in objects {
            let o = object.data;
            if let Some(o) = o {
                let gas_coin = GasCoin::try_from(&o)?;
                values_objects.push((gas_coin.value(), o.clone()));
            }
        }

        Ok(values_objects)
    }

    pub async fn get_object_owner(&self, id: &ObjectID) -> Result<SuiAddress, anyhow::Error> {
        let client = self.get_client().await?;
        let object = client
            .read_api()
            .get_object_with_options(*id, SuiObjectDataOptions::new().with_owner())
            .await?
            .into_object()?;
        Ok(object
            .owner
            .ok_or_else(|| anyhow!("Owner field is None"))?
            .get_owner_address()?)
    }

    pub async fn try_get_object_owner(
        &self,
        id: &Option<ObjectID>,
    ) -> Result<Option<SuiAddress>, anyhow::Error> {
        if let Some(id) = id {
            Ok(Some(self.get_object_owner(id).await?))
        } else {
            Ok(None)
        }
    }

    /// Infer the sender of a transaction based on the gas objects provided. If no gas objects are
    /// provided, assume the active address is the sender.
    pub async fn infer_sender(&mut self, gas: &[ObjectID]) -> Result<SuiAddress, anyhow::Error> {
        if gas.is_empty() {
            return self.active_address();
        }

        // Find the owners of all supplied object IDs
        let owners = future::try_join_all(gas.iter().map(|id| self.get_object_owner(id))).await?;

        // SAFETY `gas` is non-empty.
        let owner = owners.first().copied().unwrap();

        ensure!(
            owners.iter().all(|o| o == &owner),
            "Cannot infer sender, not all gas objects have the same owner."
        );

        Ok(owner)
    }

    /// Find a gas object which fits the budget
    pub async fn gas_for_owner_budget(
        &self,
        address: SuiAddress,
        budget: u64,
        forbidden_gas_objects: BTreeSet<ObjectID>,
    ) -> Result<(u64, SuiObjectData), anyhow::Error> {
        for o in self.gas_objects(address).await? {
            if o.0 >= budget && !forbidden_gas_objects.contains(&o.1.object_id) {
                return Ok((o.0, o.1));
            }
        }
        Err(anyhow!(
            "No non-argument gas objects found for this address with value >= budget {budget}. Run sui client gas to check for gas objects."
        ))
    }

    pub async fn get_all_gas_objects_owned_by_address(
        &self,
        address: SuiAddress,
    ) -> anyhow::Result<Vec<ObjectRef>> {
        self.get_gas_objects_owned_by_address(address, None).await
    }

    pub async fn get_gas_objects_owned_by_address(
        &self,
        address: SuiAddress,
        limit: Option<usize>,
    ) -> anyhow::Result<Vec<ObjectRef>> {
        let client = self.get_client().await?;
        let results: Vec<_> = client
            .read_api()
            .get_owned_objects(
                address,
                Some(SuiObjectResponseQuery::new(
                    Some(SuiObjectDataFilter::StructType(GasCoin::type_())),
                    Some(SuiObjectDataOptions::full_content()),
                )),
                None,
                limit,
            )
            .await?
            .data
            .into_iter()
            .filter_map(|r| r.data.map(|o| o.object_ref()))
            .collect();
        Ok(results)
    }

    /// Given an address, return one gas object owned by this address.
    /// The actual implementation just returns the first one returned by the read api.
    pub async fn get_one_gas_object_owned_by_address(
        &self,
        address: SuiAddress,
    ) -> anyhow::Result<Option<ObjectRef>> {
        Ok(self
            .get_gas_objects_owned_by_address(address, Some(1))
            .await?
            .pop())
    }

    /// Returns one address and all gas objects owned by that address.
    pub async fn get_one_account(&self) -> anyhow::Result<(SuiAddress, Vec<ObjectRef>)> {
        let address = self.get_addresses().pop().unwrap();
        Ok((
            address,
            self.get_all_gas_objects_owned_by_address(address).await?,
        ))
    }

    /// Return a gas object owned by an arbitrary address managed by the wallet.
    pub async fn get_one_gas_object(&self) -> anyhow::Result<Option<(SuiAddress, ObjectRef)>> {
        for address in self.get_addresses() {
            if let Some(gas_object) = self.get_one_gas_object_owned_by_address(address).await? {
                return Ok(Some((address, gas_object)));
            }
        }
        Ok(None)
    }

    /// Returns all the account addresses managed by the wallet and their owned gas objects.
    pub async fn get_all_accounts_and_gas_objects(
        &self,
    ) -> anyhow::Result<Vec<(SuiAddress, Vec<ObjectRef>)>> {
        let mut result = vec![];
        for address in self.get_addresses() {
            let objects = self
                .gas_objects(address)
                .await?
                .into_iter()
                .map(|(_, o)| o.object_ref())
                .collect();
            result.push((address, objects));
        }
        Ok(result)
    }

    pub async fn get_reference_gas_price(&self) -> Result<u64, anyhow::Error> {
        let client = self.get_client().await?;
        let gas_price = client.governance_api().get_reference_gas_price().await?;
        Ok(gas_price)
    }

    /// Add an account
    pub async fn add_account(&mut self, alias: Option<String>, keypair: SuiKeyPair) {
        self.config.keystore.import(alias, keypair).await.unwrap();
    }

    pub fn get_keystore_by_identity(
        &self,
        key_identity: &KeyIdentity,
    ) -> Result<&Keystore, anyhow::Error> {
        if self.config.keystore.get_by_identity(key_identity).is_ok() {
            return Ok(&self.config.keystore);
        }

        if let Some(external_keys) = self.config.external_keys.as_ref() {
            if external_keys.get_by_identity(key_identity).is_ok() {
                return Ok(external_keys);
            }
        }

        Err(anyhow!(
            "No keystore found for the provided key identity: {key_identity}"
        ))
    }

    pub fn get_keystore_by_identity_mut(
        &mut self,
        key_identity: &KeyIdentity,
    ) -> Result<&mut Keystore, anyhow::Error> {
        if self.config.keystore.get_by_identity(key_identity).is_ok() {
            return Ok(&mut self.config.keystore);
        }

        if let Some(external_keys) = self.config.external_keys.as_mut() {
            if external_keys.get_by_identity(key_identity).is_ok() {
                return Ok(external_keys);
            }
        }

        Err(anyhow!(
            "No keystore found for the provided key identity: {key_identity}"
        ))
    }

    pub async fn sign_secure(
        &self,
        key_identity: &KeyIdentity,
        data: &TransactionData,
        intent: Intent,
    ) -> Result<Signature, anyhow::Error> {
        let keystore = self.get_keystore_by_identity(key_identity)?;
        let sig = keystore.sign_secure(&data.sender(), data, intent).await?;
        Ok(sig)
    }

    /// Sign a transaction with a key currently managed by the WalletContext
    pub async fn sign_transaction(&self, data: &TransactionData) -> Transaction {
        let sig = self
            .config
            .keystore
            .sign_secure(&data.sender(), data, Intent::sui_transaction())
            .await
            .unwrap();
        // TODO: To support sponsored transaction, we should also look at the gas owner.
        Transaction::from_data(data.clone(), vec![sig])
    }

    /// Execute a transaction and wait for it to be locally executed on the fullnode.
    /// Also expects the effects status to be ExecutionStatus::Success.
    pub async fn execute_transaction_must_succeed(
        &self,
        tx: Transaction,
    ) -> SuiTransactionBlockResponse {
        tracing::debug!("Executing transaction: {:?}", tx);
        let response = self.execute_transaction_may_fail(tx).await.unwrap();
        assert!(
            response.status_ok().unwrap(),
            "Transaction failed: {:?}",
            response
        );
        response
    }

    /// Execute a transaction and wait for it to be locally executed on the fullnode.
    /// The transaction execution is not guaranteed to succeed and may fail. This is usually only
    /// needed in non-test environment or the caller is explicitly testing some failure behavior.
    pub async fn execute_transaction_may_fail(
        &self,
        tx: Transaction,
    ) -> anyhow::Result<SuiTransactionBlockResponse> {
        let client = self.get_client().await?;
        Ok(client
            .quorum_driver_api()
            .execute_transaction_block(
                tx,
                SuiTransactionBlockResponseOptions::new()
                    .with_effects()
                    .with_input()
                    .with_events()
                    .with_object_changes()
                    .with_balance_changes(),
                Some(sui_types::quorum_driver_types::ExecuteTransactionRequestType::WaitForLocalExecution),
            )
            .await?)
    }
}
