// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// For testing, use existing RPC as data source

use crate::error::Error;
use crate::types::address::Address;
use crate::types::base64::Base64;
use crate::types::big_int::BigInt;
use crate::types::date_time::DateTime;
use crate::types::digest::Digest;
use crate::types::epoch::Epoch;
use crate::types::object::{Object, ObjectKind};
use crate::types::protocol_config::{
    ProtocolConfigAttr, ProtocolConfigFeatureFlag, ProtocolConfigs,
};
use crate::types::sui_address::SuiAddress;
use crate::types::transaction_block::TransactionBlock;
use crate::types::validator::Validator;
use crate::types::validator_credentials::ValidatorCredentials;
use crate::types::validator_set::ValidatorSet;

use async_graphql::dataloader::*;
use async_graphql::*;
use async_trait::async_trait;
use std::collections::HashMap;
use std::time::Duration;
use sui_json_rpc_types::{SuiObjectDataOptions, SuiRawData, SuiTransactionBlockResponseOptions};
use sui_sdk::types::digests::TransactionDigest;
use sui_sdk::types::sui_system_state::sui_system_state_summary::SuiSystemStateSummary;
use sui_sdk::{
    types::{
        base_types::{ObjectID as NativeObjectID, SuiAddress as NativeSuiAddress},
        object::Owner as NativeOwner,
        sui_system_state::sui_system_state_summary::SuiValidatorSummary,
    },
    SuiClient,
};

use super::data_provider::DataProvider;

// TODO: Ensure the logic is this file is for verification/experimentation only
// and is not used in production

const RPC_TIMEOUT_ERR_SLEEP_RETRY_PERIOD: Duration = Duration::from_millis(10_000);
const MAX_CONCURRENT_REQUESTS: usize = 1_000;
const DATA_LOADER_LRU_CACHE_SIZE: usize = 1_000;

pub(crate) struct SuiClientLoader {
    pub client: SuiClient,
}

#[async_trait::async_trait]
impl Loader<Digest> for SuiClientLoader {
    type Value = TransactionBlock;
    type Error = async_graphql::Error;

    async fn load(&self, keys: &[Digest]) -> Result<HashMap<Digest, Self::Value>, Self::Error> {
        let mut map = HashMap::new();
        let keys: Vec<_> = keys
            .iter()
            .map(|x| TransactionDigest::new(x.into_array()))
            .collect();
        for tx in self
            .client
            .read_api()
            .multi_get_transactions_with_options(
                keys,
                SuiTransactionBlockResponseOptions::full_content(),
            )
            .await?
        {
            let digest = Digest::from_array(tx.digest.into_inner());
            let mtx = TransactionBlock::from(tx);
            map.insert(digest, mtx);
        }
        Ok(map)
    }
}

#[async_trait]
impl DataProvider for SuiClient {
    async fn get_object_with_options(
        &self,
        object_id: NativeObjectID,
        options: SuiObjectDataOptions,
    ) -> Result<Option<Object>> {
        let obj = self
            .read_api()
            .get_object_with_options(object_id, options)
            .await?;

        if obj.error.is_some() || obj.data.is_none() {
            return Ok(None);
        }
        Ok(Some(convert_obj(&obj.data.unwrap())))
    }

    async fn multi_get_object_with_options(
        &self,
        object_ids: Vec<NativeObjectID>,
        options: SuiObjectDataOptions,
    ) -> Result<Vec<Object>> {
        let obj_responses = self
            .read_api()
            .multi_get_object_with_options(object_ids, options)
            .await?;

        let mut objs = Vec::new();

        for n in obj_responses.iter() {
            if n.error.is_some() {
                return Err(Error::MultiGet(n.error.as_ref().unwrap().to_string()).extend());
            } else if n.data.is_none() {
                return Err(Error::Internal(
                    "Expected either data or error fields, received neither".to_string(),
                )
                .extend());
            }
            objs.push(convert_obj(n.data.as_ref().unwrap()));
        }
        Ok(objs)
    }

    async fn fetch_protocol_config(&self, version: Option<u64>) -> Result<ProtocolConfigs> {
        let cfg = self
            .read_api()
            .get_protocol_config(version.map(|x| x.into()))
            .await?;

        Ok(ProtocolConfigs {
            configs: cfg
                .attributes
                .into_iter()
                .map(|(k, v)| ProtocolConfigAttr {
                    key: k,
                    // TODO:  what to return when value is None? nothing?
                    // TODO: do we want to return type info separately?
                    value: match v {
                        Some(q) => format!("{:?}", q),
                        None => "".to_string(),
                    },
                })
                .collect(),
            feature_flags: cfg
                .feature_flags
                .into_iter()
                .map(|x| ProtocolConfigFeatureFlag {
                    key: x.0,
                    value: x.1,
                })
                .collect(),
            protocol_version: cfg.protocol_version.as_u64(),
        })
    }

    async fn get_latest_sui_system_state(&self) -> Result<SuiSystemStateSummary> {
        Ok(self.governance_api().get_latest_sui_system_state().await?)
    }
}

pub(crate) async fn sui_sdk_client_v0(rpc_url: impl AsRef<str>) -> SuiClient {
    sui_sdk::SuiClientBuilder::default()
        .request_timeout(RPC_TIMEOUT_ERR_SLEEP_RETRY_PERIOD)
        .max_concurrent_requests(MAX_CONCURRENT_REQUESTS)
        .build(rpc_url)
        .await
        .expect("Failed to create SuiClient")
}

pub(crate) async fn lru_cache_data_loader(
    client: &SuiClient,
) -> DataLoader<SuiClientLoader, LruCache> {
    let data_loader = DataLoader::with_cache(
        SuiClientLoader {
            client: client.clone(),
        },
        tokio::spawn,
        async_graphql::dataloader::LruCache::new(DATA_LOADER_LRU_CACHE_SIZE),
    );
    data_loader.enable_all_cache(true);
    data_loader
}

pub(crate) fn convert_obj(s: &sui_json_rpc_types::SuiObjectData) -> Object {
    Object {
        version: s.version.into(),
        digest: s.digest.base58_encode(),
        storage_rebate: s.storage_rebate.map(BigInt::from),
        address: SuiAddress::from_array(**s.object_id),
        owner: s
            .owner
            .unwrap()
            .get_owner_address()
            .map(|x| SuiAddress::from_array(x.to_inner()))
            .ok(),
        bcs: s.bcs.as_ref().map(|raw| match raw {
            SuiRawData::Package(raw_package) => Base64::from(bcs::to_bytes(raw_package).unwrap()),
            SuiRawData::MoveObject(raw_object) => Base64::from(&raw_object.bcs_bytes),
        }),
        previous_transaction: s
            .previous_transaction
            .map(|x| Digest::from_array(x.into_inner())),
        kind: Some(match s.owner.unwrap() {
            NativeOwner::AddressOwner(_) => ObjectKind::Owned,
            NativeOwner::ObjectOwner(_) => ObjectKind::Child,
            NativeOwner::Shared {
                initial_shared_version: _,
            } => ObjectKind::Shared,
            NativeOwner::Immutable => ObjectKind::Immutable,
        }),
    }
}

pub(crate) fn _convert_to_epoch(
    system_state: &SuiSystemStateSummary,
    protocol_configs: &ProtocolConfigs,
) -> Result<Epoch> {
    let epoch_id = system_state.epoch;
    let active_validators =
        convert_to_validators(system_state.active_validators.clone(), Some(system_state));
    let start_timestamp = i64::try_from(system_state.epoch_start_timestamp_ms).map_err(|_| {
        Error::Internal(format!(
            "Cannot convert start timestamp u64 ({}) of epoch ({epoch_id}) into i64 required by DateTime",
            system_state.epoch_start_timestamp_ms
        ))
    })?;

    let start_timestamp = DateTime::from_ms(start_timestamp).ok_or_else(|| {
        Error::Internal(format!(
            "Cannot convert start timestamp ({}) of epoch ({epoch_id}) into a DateTime",
            start_timestamp
        ))
    })?;

    Ok(Epoch {
        epoch_id,
        reference_gas_price: Some(BigInt::from(system_state.reference_gas_price)),
        validator_set: Some(ValidatorSet {
            total_stake: Some(BigInt::from(system_state.total_stake)),
            active_validators: Some(active_validators),
            pending_removals: Some(system_state.pending_removals.clone()),
            pending_active_validators_size: Some(system_state.pending_active_validators_size),
            stake_pool_mappings_size: Some(system_state.staking_pool_mappings_size),
            inactive_pools_size: Some(system_state.inactive_pools_size),
            validator_candidates_size: Some(system_state.validator_candidates_size),
        }),
        protocol_version: protocol_configs.protocol_version,
        start_timestamp: Some(start_timestamp),
        end_timestamp: None,
    })
}

pub(crate) fn convert_to_validators(
    validators: Vec<SuiValidatorSummary>,
    system_state: Option<&SuiSystemStateSummary>,
) -> Vec<Validator> {
    validators
        .iter()
        .map(|v| {
            let at_risk = system_state
                .and_then(|system_state| {
                    system_state
                        .at_risk_validators
                        .iter()
                        .find(|&(address, _)| address == &v.sui_address)
                })
                .map(|&(_, value)| value);

            let report_records = system_state
                .and_then(|system_state| {
                    system_state
                        .validator_report_records
                        .iter()
                        .find(|&(address, _)| address == &v.sui_address)
                })
                .map(|(_, value)| {
                    value
                        .iter()
                        .map(|address| SuiAddress::from_array(address.to_inner()))
                        .collect::<Vec<_>>()
                });

            let credentials = ValidatorCredentials {
                protocol_pub_key: Some(Base64::from(v.protocol_pubkey_bytes.clone())),
                network_pub_key: Some(Base64::from(v.network_pubkey_bytes.clone())),
                worker_pub_key: Some(Base64::from(v.worker_pubkey_bytes.clone())),
                proof_of_possession: Some(Base64::from(v.proof_of_possession_bytes.clone())),
                net_address: Some(v.net_address.clone()),
                p2p_address: Some(v.p2p_address.clone()),
                primary_address: Some(v.primary_address.clone()),
                worker_address: Some(v.worker_address.clone()),
            };
            Validator {
                address: Address {
                    address: SuiAddress::from(v.sui_address),
                },
                next_epoch_credentials: Some(credentials.clone()),
                credentials: Some(credentials),
                name: Some(v.name.clone()),
                description: Some(v.description.clone()),
                image_url: Some(v.image_url.clone()),
                project_url: Some(v.project_url.clone()),

                operation_cap_id: SuiAddress::from_array(**v.operation_cap_id),
                staking_pool_id: SuiAddress::from_array(**v.staking_pool_id),
                exchange_rates_id: SuiAddress::from_array(**v.exchange_rates_id),
                exchange_rates_size: Some(v.exchange_rates_size),

                staking_pool_activation_epoch: v.staking_pool_activation_epoch,
                staking_pool_sui_balance: Some(BigInt::from(v.staking_pool_sui_balance)),
                rewards_pool: Some(BigInt::from(v.rewards_pool)),
                pool_token_balance: Some(BigInt::from(v.pool_token_balance)),
                pending_stake: Some(BigInt::from(v.pending_stake)),
                pending_total_sui_withdraw: Some(BigInt::from(v.pending_total_sui_withdraw)),
                pending_pool_token_withdraw: Some(BigInt::from(v.pending_pool_token_withdraw)),
                voting_power: Some(v.voting_power),
                // stake_units: todo!(),
                gas_price: Some(BigInt::from(v.gas_price)),
                commission_rate: Some(v.commission_rate),
                next_epoch_stake: Some(BigInt::from(v.next_epoch_stake)),
                next_epoch_gas_price: Some(BigInt::from(v.next_epoch_gas_price)),
                next_epoch_commission_rate: Some(v.next_epoch_commission_rate),
                at_risk,
                report_records,
                // apy: todo!(),
            }
        })
        .collect()
}

impl From<Address> for SuiAddress {
    fn from(a: Address) -> Self {
        a.address
    }
}

impl From<SuiAddress> for Address {
    fn from(a: SuiAddress) -> Self {
        Address { address: a }
    }
}

impl From<NativeSuiAddress> for SuiAddress {
    fn from(a: NativeSuiAddress) -> Self {
        SuiAddress::from_array(a.to_inner())
    }
}

impl From<SuiAddress> for NativeSuiAddress {
    fn from(a: SuiAddress) -> Self {
        NativeSuiAddress::try_from(a.as_slice()).unwrap()
    }
}

impl From<&SuiAddress> for NativeSuiAddress {
    fn from(a: &SuiAddress) -> Self {
        NativeSuiAddress::try_from(a.as_slice()).unwrap()
    }
}
