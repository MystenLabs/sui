// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// For testing, use existing RPC as data source

use crate::error::Error;
use crate::types::address::Address;
use crate::types::balance::Balance;
use crate::types::base64::Base64;
use crate::types::big_int::BigInt;
use crate::types::checkpoint::Checkpoint;
use crate::types::committee_member::CommitteeMember;
use crate::types::date_time::DateTime;
use crate::types::digest::Digest;
use crate::types::end_of_epoch_data::EndOfEpochData;
use crate::types::epoch::Epoch;
use crate::types::object::{Object, ObjectFilter, ObjectKind};
use crate::types::protocol_config::{
    ProtocolConfigAttr, ProtocolConfigFeatureFlag, ProtocolConfigs,
};
use crate::types::safe_mode::SafeMode;
use crate::types::stake_subsidy::StakeSubsidy;
use crate::types::storage_fund::StorageFund;
use crate::types::sui_address::SuiAddress;
use crate::types::system_parameters::SystemParameters;
use crate::types::transaction_block::TransactionBlock;
use crate::types::validator::Validator;
use crate::types::validator_credentials::ValidatorCredentials;
use crate::types::validator_set::ValidatorSet;

use crate::types::gas::GasCostSummary;
use async_graphql::connection::{Connection, Edge};
use async_graphql::dataloader::*;
use async_graphql::*;
use async_trait::async_trait;
use fastcrypto::traits::EncodeDecodeBase64;
use std::collections::HashMap;
use std::str::FromStr;
use std::time::Duration;
use sui_json_rpc_types::{
    SuiObjectDataOptions, SuiObjectResponseQuery, SuiPastObjectResponse, SuiRawData,
    SuiTransactionBlockResponseOptions,
};
use sui_sdk::types::digests::TransactionDigest;
use sui_sdk::types::sui_serde::BigInt as SerdeBigInt;
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

const RPC_TIMEOUT_ERR_SLEEP_RETRY_PERIOD: Duration = Duration::from_millis(10_000);
const MAX_CONCURRENT_REQUESTS: usize = 1_000;
const DATA_LOADER_LRU_CACHE_SIZE: usize = 1_000;

const DEFAULT_PAGE_SIZE: usize = 50;

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
    async fn fetch_obj(&self, address: SuiAddress, version: Option<u64>) -> Result<Option<Object>> {
        let oid: NativeObjectID = address.into_array().as_slice().try_into()?;
        let opts = SuiObjectDataOptions::full_content();

        let g = match version {
            Some(v) => match self
                .read_api()
                .try_get_parsed_past_object(oid, v.into(), opts)
                .await?
            {
                SuiPastObjectResponse::VersionFound(x) => x,
                _ => return Ok(None),
            },
            None => {
                let val = self.read_api().get_object_with_options(oid, opts).await?;
                if val.error.is_some() || val.data.is_none() {
                    return Ok(None);
                }
                val.data.unwrap()
            }
        };
        Ok(Some(convert_obj(&g)))
    }

    async fn fetch_owned_objs(
        &self,
        owner: &SuiAddress,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
        _filter: Option<ObjectFilter>,
    ) -> Result<Connection<String, Object>> {
        ensure_forward_pagination(&first, &after, &last, &before)?;

        let count = first.map(|q| q as usize);
        let native_owner = NativeSuiAddress::from(owner);
        let query = SuiObjectResponseQuery::new_with_options(SuiObjectDataOptions::full_content());

        let cursor = match after {
            Some(q) => Some(
                NativeObjectID::from_hex_literal(&q)
                    .map_err(|w| Error::InvalidCursor(w.to_string()).extend())?,
            ),
            None => None,
        };

        let pg = self
            .read_api()
            .get_owned_objects(native_owner, Some(query), cursor, count)
            .await?;

        // TODO: support partial success/ failure responses
        pg.data.iter().try_for_each(|n| {
            if n.error.is_some() {
                return Err(Error::CursorConnectionFetchFailed(
                    n.error.as_ref().unwrap().to_string(),
                )
                .extend());
            } else if n.data.is_none() {
                return Err(Error::Internal(
                    "Expected either data or error fields, received neither".to_string(),
                )
                .extend());
            }
            Ok(())
        })?;
        let mut connection = Connection::new(false, pg.has_next_page);

        connection.edges.extend(pg.data.into_iter().map(|n| {
            let g = n.data.unwrap();
            let o = convert_obj(&g);

            Edge::new(g.object_id.to_string(), o)
        }));
        Ok(connection)
    }

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

    async fn fetch_balance(&self, address: &SuiAddress, type_: Option<String>) -> Result<Balance> {
        let b = self
            .coin_read_api()
            .get_balance(address.into(), type_)
            .await?;
        Ok(convert_bal(b))
    }

    async fn fetch_balance_connection(
        &self,
        address: &SuiAddress,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
    ) -> Result<Connection<String, Balance>> {
        ensure_forward_pagination(&first, &after, &last, &before)?;

        let count = first.unwrap_or(DEFAULT_PAGE_SIZE as u64) as usize;
        let offset = after
            .map(|q| q.parse::<usize>().unwrap())
            .unwrap_or(0_usize);

        // This fetches all balances but we only want a slice
        // The pagination logic here can break if data is added
        // This is okay for now as we're only using this for testing
        let balances = self
            .coin_read_api()
            .get_all_balances(NativeSuiAddress::from(address))
            .await?;

        let max = balances.len();

        let bs = balances.into_iter().skip(offset).take(count);

        let mut connection = Connection::new(false, offset + count < max);

        connection
            .edges
            .extend(bs.into_iter().enumerate().map(|(i, b)| {
                let balance = convert_bal(b);
                Edge::new(format!("{:032}", offset + i), balance)
            }));
        Ok(connection)
    }

    // TODO: support backward pagination as fetching checkpoints
    // API allows for it
    async fn fetch_checkpoint_connection(
        &self,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
    ) -> Result<Connection<String, Checkpoint>> {
        ensure_forward_pagination(&first, &after, &last, &before)?;

        let count = first.map(|q| q as usize);
        let after = after
            .map(|x| x.parse::<u64>())
            .transpose()
            .map_err(|_| {
                Error::InvalidCursor(
                    "Cannot convert after parameter into u64 in the checkpoint connection"
                        .to_string(),
                )
            })?
            .map(SerdeBigInt::from);

        let pg = self.read_api().get_checkpoints(after, count, false).await?;
        let data: Result<Vec<_>, _> = pg.data.iter().map(convert_json_rpc_checkpoint).collect();

        let checkpoints = data.map_err(|e| {
            Error::Internal(format!(
                "Cannot convert the JSON RPC checkpoint into GraphQL checkpoint type: {}",
                e.message
            ))
        })?;

        let mut connection = Connection::new(false, pg.has_next_page);
        connection.edges.extend(
            checkpoints
                .iter()
                .map(|x| Edge::new(x.sequence_number.to_string(), x.clone())),
        );

        Ok(connection)
    }

    async fn fetch_chain_id(&self) -> Result<String> {
        Ok(self.read_api().get_chain_identifier().await?)
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

pub(crate) fn convert_json_rpc_checkpoint(
    c: &sui_json_rpc_types::Checkpoint,
) -> Result<Checkpoint> {
    let digest = c.digest.to_string();
    let sequence_number = c.sequence_number;

    let validator_signature = c.validator_signature.encode_base64();
    let validator_signature = Some(Base64::from(validator_signature.into_bytes()));

    let previous_checkpoint_digest = c.previous_digest.map(|x| x.to_string());
    let network_total_transactions = Some(c.network_total_transactions);
    let rolling_gas_summary = GasCostSummary::from(&c.epoch_rolling_gas_cost_summary);

    let end_of_epoch_data = &c.end_of_epoch_data;
    let end_of_epoch = end_of_epoch_data.clone().map(|e| {
        let committees = e.next_epoch_committee;
        let new_committee = if committees.is_empty() {
            None
        } else {
            Some(
                committees
                    .iter()
                    .map(|c| CommitteeMember {
                        authority_name: Some(c.0.into_concise().to_string()),
                        stake_unit: Some(c.1),
                    })
                    .collect::<Vec<_>>(),
            )
        };

        EndOfEpochData {
            new_committee,
            next_protocol_version: Some(e.next_epoch_protocol_version.as_u64()),
        }
    });

    let timestamp = i64::try_from(c.timestamp_ms).map_err(|_| {
        Error::Internal(format!(
            "Cannot convert start timestamp u64 ({}) of checkpoint ({sequence_number}) into i64 required by DateTime",
            c.timestamp_ms
        ))
    })?;

    Ok(Checkpoint {
        digest,
        sequence_number,
        validator_signature,
        timestamp: DateTime::from_ms(timestamp),
        previous_checkpoint_digest,
        live_object_set_digest: None, // TODO fix this
        network_total_transactions,
        rolling_gas_summary: Some(rolling_gas_summary),
        epoch_id: c.epoch,
        end_of_epoch,
    })
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

fn convert_bal(b: sui_json_rpc_types::Balance) -> Balance {
    Balance {
        coin_object_count: b.coin_object_count as u64,
        total_balance: BigInt::from_str(&format!("{}", b.total_balance)).unwrap(),
    }
}

pub(crate) fn convert_to_epoch(
    gas_summary: GasCostSummary,
    system_state: &SuiSystemStateSummary,
    protocol_configs: &ProtocolConfigs,
) -> Result<Epoch> {
    let epoch_id = system_state.epoch;
    let active_validators = convert_to_validators(system_state.active_validators.clone())?;

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
        system_state_version: Some(BigInt::from(system_state.system_state_version)),
        reference_gas_price: Some(BigInt::from(system_state.reference_gas_price)),
        system_parameters: Some(SystemParameters {
            duration_ms: Some(BigInt::from(system_state.epoch_duration_ms)),
            stake_subsidy_start_epoch: Some(system_state.stake_subsidy_start_epoch),
            min_validator_count: Some(system_state.max_validator_count),
            max_validator_count: Some(system_state.max_validator_count),
            min_validator_joining_stake: Some(BigInt::from(
                system_state.min_validator_joining_stake,
            )),
            validator_low_stake_threshold: Some(BigInt::from(
                system_state.validator_low_stake_threshold,
            )),
            validator_very_low_stake_threshold: Some(BigInt::from(
                system_state.validator_very_low_stake_threshold,
            )),
            validator_low_stake_grace_period: Some(BigInt::from(
                system_state.validator_low_stake_grace_period,
            )),
        }),
        stake_subsidy: Some(StakeSubsidy {
            balance: Some(BigInt::from(system_state.stake_subsidy_balance)),
            distribution_counter: Some(system_state.stake_subsidy_distribution_counter),
            current_distribution_amount: Some(BigInt::from(
                system_state.stake_subsidy_current_distribution_amount,
            )),
            period_length: Some(system_state.stake_subsidy_period_length),
            decrease_rate: Some(system_state.stake_subsidy_decrease_rate as u64),
        }),
        validator_set: Some(ValidatorSet {
            total_stake: Some(BigInt::from(system_state.total_stake)),
            active_validators: Some(active_validators),
            pending_removals: Some(system_state.pending_removals.clone()),
            pending_active_validators_size: Some(system_state.pending_active_validators_size),
            stake_pool_mappings_size: Some(system_state.staking_pool_mappings_size),
            inactive_pools_size: Some(system_state.inactive_pools_size),
            validator_candidates_size: Some(system_state.validator_candidates_size),
        }),
        storage_fund: Some(StorageFund {
            total_object_storage_rebates: Some(BigInt::from(
                system_state.storage_fund_total_object_storage_rebates,
            )),
            non_refundable_balance: Some(BigInt::from(
                system_state.storage_fund_non_refundable_balance,
            )),
        }),
        safe_mode: Some(SafeMode {
            enabled: Some(system_state.safe_mode),
            gas_summary: Some(gas_summary),
        }),
        protocol_configs: Some(protocol_configs.clone()),
        start_timestamp: Some(start_timestamp),
    })
}

pub(crate) fn convert_to_validators(
    validators: Vec<SuiValidatorSummary>,
) -> Result<Vec<Validator>> {
    let result = validators
        .iter()
        .map(|v| {
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
                exchange_rates_size: Some(v.exchange_rates_size),

                staking_pool_activation_epoch: Some(v.staking_pool_activation_epoch.unwrap()),
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
                // at_risk: todo!(),
                // report_records: todo!(),
                // apy: todo!(),
            }
        })
        .collect();

    Ok(result)
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

fn ensure_forward_pagination(
    first: &Option<u64>,
    after: &Option<String>,
    last: &Option<u64>,
    before: &Option<String>,
) -> Result<()> {
    if before.is_some() && after.is_some() {
        return Err(Error::CursorNoBeforeAfter.extend());
    }
    if first.is_some() && last.is_some() {
        return Err(Error::CursorNoFirstLast.extend());
    }
    if before.is_some() || last.is_some() {
        return Err(Error::CursorNoReversePagination.extend());
    }
    Ok(())
}
