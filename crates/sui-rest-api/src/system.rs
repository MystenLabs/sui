// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    accept::AcceptFormat,
    openapi::{ApiEndpoint, RouteHandler},
    reader::StateReader,
    RestError, RestService, Result,
};
use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use sui_protocol_config::{ProtocolConfig, ProtocolConfigValue, ProtocolVersion};
use sui_sdk2::types::{Address, ObjectId};

pub struct GetSystemStateSummary;

impl ApiEndpoint<RestService> for GetSystemStateSummary {
    fn method(&self) -> axum::http::Method {
        axum::http::Method::GET
    }

    fn path(&self) -> &'static str {
        "/system"
    }

    fn operation(
        &self,
        _generator: &mut schemars::gen::SchemaGenerator,
    ) -> openapiv3::v3_1::Operation {
        openapiv3::v3_1::Operation::default()
    }

    fn handler(&self) -> RouteHandler<RestService> {
        RouteHandler::new(self.method(), get_system_state_summary)
    }
}

async fn get_system_state_summary(
    accept: AcceptFormat,
    State(state): State<StateReader>,
) -> Result<Json<SystemStateSummary>> {
    match accept {
        AcceptFormat::Json => {}
        _ => {
            return Err(RestError::new(
                axum::http::StatusCode::BAD_REQUEST,
                "invalid accept type",
            ))
        }
    }

    let summary = state.get_system_state_summary()?;

    Ok(Json(summary))
}

#[serde_with::serde_as]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SystemStateSummary {
    /// The current epoch ID, starting from 0.
    #[serde_as(as = "sui_types::sui_serde::BigInt<u64>")]
    pub epoch: u64,
    /// The current protocol version, starting from 1.
    #[serde_as(as = "sui_types::sui_serde::BigInt<u64>")]
    pub protocol_version: u64,
    /// The current version of the system state data structure type.
    #[serde_as(as = "sui_types::sui_serde::BigInt<u64>")]
    pub system_state_version: u64,
    /// The storage rebates of all the objects on-chain stored in the storage fund.
    #[serde_as(as = "sui_types::sui_serde::BigInt<u64>")]
    pub storage_fund_total_object_storage_rebates: u64,
    /// The non-refundable portion of the storage fund coming from storage reinvestment, non-refundable
    /// storage rebates and any leftover staking rewards.
    #[serde_as(as = "sui_types::sui_serde::BigInt<u64>")]
    pub storage_fund_non_refundable_balance: u64,
    /// The reference gas price for the current epoch.
    #[serde_as(as = "sui_types::sui_serde::BigInt<u64>")]
    pub reference_gas_price: u64,
    /// Whether the system is running in a downgraded safe mode due to a non-recoverable bug.
    /// This is set whenever we failed to execute advance_epoch, and ended up executing advance_epoch_safe_mode.
    /// It can be reset once we are able to successfully execute advance_epoch.
    pub safe_mode: bool,
    /// Amount of storage rewards accumulated (and not yet distributed) during safe mode.
    #[serde_as(as = "sui_types::sui_serde::BigInt<u64>")]
    pub safe_mode_storage_rewards: u64,
    /// Amount of computation rewards accumulated (and not yet distributed) during safe mode.
    #[serde_as(as = "sui_types::sui_serde::BigInt<u64>")]
    pub safe_mode_computation_rewards: u64,
    /// Amount of storage rebates accumulated (and not yet burned) during safe mode.
    #[serde_as(as = "sui_types::sui_serde::BigInt<u64>")]
    pub safe_mode_storage_rebates: u64,
    /// Amount of non-refundable storage fee accumulated during safe mode.
    #[serde_as(as = "sui_types::sui_serde::BigInt<u64>")]
    pub safe_mode_non_refundable_storage_fee: u64,
    /// Unix timestamp of the current epoch start
    #[serde_as(as = "sui_types::sui_serde::BigInt<u64>")]
    pub epoch_start_timestamp_ms: u64,

    // System parameters
    /// The duration of an epoch, in milliseconds.
    #[serde_as(as = "sui_types::sui_serde::BigInt<u64>")]
    pub epoch_duration_ms: u64,

    /// The starting epoch in which stake subsidies start being paid out
    #[serde_as(as = "sui_types::sui_serde::BigInt<u64>")]
    pub stake_subsidy_start_epoch: u64,

    /// Maximum number of active validators at any moment.
    /// We do not allow the number of validators in any epoch to go above this.
    #[serde_as(as = "sui_types::sui_serde::BigInt<u64>")]
    pub max_validator_count: u64,

    /// Lower-bound on the amount of stake required to become a validator.
    #[serde_as(as = "sui_types::sui_serde::BigInt<u64>")]
    pub min_validator_joining_stake: u64,

    /// Validators with stake amount below `validator_low_stake_threshold` are considered to
    /// have low stake and will be escorted out of the validator set after being below this
    /// threshold for more than `validator_low_stake_grace_period` number of epochs.
    #[serde_as(as = "sui_types::sui_serde::BigInt<u64>")]
    pub validator_low_stake_threshold: u64,

    /// Validators with stake below `validator_very_low_stake_threshold` will be removed
    /// immediately at epoch change, no grace period.
    #[serde_as(as = "sui_types::sui_serde::BigInt<u64>")]
    pub validator_very_low_stake_threshold: u64,

    /// A validator can have stake below `validator_low_stake_threshold`
    /// for this many epochs before being kicked out.
    #[serde_as(as = "sui_types::sui_serde::BigInt<u64>")]
    pub validator_low_stake_grace_period: u64,

    // Stake subsidy information
    /// Balance of SUI set aside for stake subsidies that will be drawn down over time.
    #[serde_as(as = "sui_types::sui_serde::BigInt<u64>")]
    pub stake_subsidy_balance: u64,
    /// This counter may be different from the current epoch number if
    /// in some epochs we decide to skip the subsidy.
    #[serde_as(as = "sui_types::sui_serde::BigInt<u64>")]
    pub stake_subsidy_distribution_counter: u64,
    /// The amount of stake subsidy to be drawn down per epoch.
    /// This amount decays and decreases over time.
    #[serde_as(as = "sui_types::sui_serde::BigInt<u64>")]
    pub stake_subsidy_current_distribution_amount: u64,
    /// Number of distributions to occur before the distribution amount decays.
    #[serde_as(as = "sui_types::sui_serde::BigInt<u64>")]
    pub stake_subsidy_period_length: u64,
    /// The rate at which the distribution amount decays at the end of each
    /// period. Expressed in basis points.
    pub stake_subsidy_decrease_rate: u16,

    // Validator set
    /// Total amount of stake from all active validators at the beginning of the epoch.
    #[serde_as(as = "sui_types::sui_serde::BigInt<u64>")]
    pub total_stake: u64,
    /// The list of active validators in the current epoch.
    pub active_validators: Vec<ValidatorSummary>,
    /// ID of the object that contains the list of new validators that will join at the end of the epoch.
    pub pending_active_validators_id: ObjectId,
    /// Number of new validators that will join at the end of the epoch.
    #[serde_as(as = "sui_types::sui_serde::BigInt<u64>")]
    pub pending_active_validators_size: u64,
    /// Removal requests from the validators. Each element is an index
    /// pointing to `active_validators`.
    #[serde_as(as = "Vec<sui_types::sui_serde::BigInt<u64>>")]
    pub pending_removals: Vec<u64>,
    /// ID of the object that maps from staking pool's ID to the sui address of a validator.
    pub staking_pool_mappings_id: ObjectId,
    /// Number of staking pool mappings.
    #[serde_as(as = "sui_types::sui_serde::BigInt<u64>")]
    pub staking_pool_mappings_size: u64,
    /// ID of the object that maps from a staking pool ID to the inactive validator that has that pool as its staking pool.
    pub inactive_pools_id: ObjectId,
    /// Number of inactive staking pools.
    #[serde_as(as = "sui_types::sui_serde::BigInt<u64>")]
    pub inactive_pools_size: u64,
    /// ID of the object that stores preactive validators, mapping their addresses to their `Validator` structs.
    pub validator_candidates_id: ObjectId,
    /// Number of preactive validators.
    #[serde_as(as = "sui_types::sui_serde::BigInt<u64>")]
    pub validator_candidates_size: u64,
    /// Map storing the number of epochs for which each validator has been below the low stake threshold.
    #[serde_as(as = "Vec<(_, sui_types::sui_serde::BigInt<u64>)>")]
    pub at_risk_validators: Vec<(Address, u64)>,
    /// A map storing the records of validator reporting each other.
    pub validator_report_records: Vec<(Address, Vec<Address>)>,
}

/// This is the REST type for the sui validator. It flattens all inner structures
/// to top-level fields so that they are decoupled from the internal definitions.
#[serde_with::serde_as]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ValidatorSummary {
    // Metadata
    pub address: Address,
    pub protocol_public_key: sui_sdk2::types::Bls12381PublicKey,
    pub network_public_key: sui_sdk2::types::Ed25519PublicKey,
    pub worker_public_key: sui_sdk2::types::Ed25519PublicKey,
    #[serde_as(as = "fastcrypto::encoding::Base64")]
    pub proof_of_possession_bytes: Vec<u8>,
    pub name: String,
    pub description: String,
    pub image_url: String,
    pub project_url: String,
    pub net_address: String,
    pub p2p_address: String,
    pub primary_address: String,
    pub worker_address: String,
    pub next_epoch_protocol_public_key: Option<sui_sdk2::types::Bls12381PublicKey>,
    pub next_epoch_network_public_key: Option<sui_sdk2::types::Ed25519PublicKey>,
    pub next_epoch_worker_public_key: Option<sui_sdk2::types::Ed25519PublicKey>,
    #[serde_as(as = "Option<fastcrypto::encoding::Base64>")]
    pub next_epoch_proof_of_possession: Option<Vec<u8>>,
    pub next_epoch_net_address: Option<String>,
    pub next_epoch_p2p_address: Option<String>,
    pub next_epoch_primary_address: Option<String>,
    pub next_epoch_worker_address: Option<String>,

    #[serde_as(as = "sui_types::sui_serde::BigInt<u64>")]
    pub voting_power: u64,
    pub operation_cap_id: ObjectId,
    #[serde_as(as = "sui_types::sui_serde::BigInt<u64>")]
    pub gas_price: u64,
    #[serde_as(as = "sui_types::sui_serde::BigInt<u64>")]
    pub commission_rate: u64,
    #[serde_as(as = "sui_types::sui_serde::BigInt<u64>")]
    pub next_epoch_stake: u64,
    #[serde_as(as = "sui_types::sui_serde::BigInt<u64>")]
    pub next_epoch_gas_price: u64,
    #[serde_as(as = "sui_types::sui_serde::BigInt<u64>")]
    pub next_epoch_commission_rate: u64,

    // Staking pool information
    /// ID of the staking pool object.
    pub staking_pool_id: ObjectId,
    /// The epoch at which this pool became active.
    #[serde_as(as = "Option<sui_types::sui_serde::BigInt<u64>>")]
    pub staking_pool_activation_epoch: Option<u64>,
    /// The epoch at which this staking pool ceased to be active. `None` = {pre-active, active},
    #[serde_as(as = "Option<sui_types::sui_serde::BigInt<u64>>")]
    pub staking_pool_deactivation_epoch: Option<u64>,
    /// The total number of SUI tokens in this pool.
    #[serde_as(as = "sui_types::sui_serde::BigInt<u64>")]
    pub staking_pool_sui_balance: u64,
    /// The epoch stake rewards will be added here at the end of each epoch.
    #[serde_as(as = "sui_types::sui_serde::BigInt<u64>")]
    pub rewards_pool: u64,
    /// Total number of pool tokens issued by the pool.
    #[serde_as(as = "sui_types::sui_serde::BigInt<u64>")]
    pub pool_token_balance: u64,
    /// Pending stake amount for this epoch.
    #[serde_as(as = "sui_types::sui_serde::BigInt<u64>")]
    pub pending_stake: u64,
    /// Pending stake withdrawn during the current epoch, emptied at epoch boundaries.
    #[serde_as(as = "sui_types::sui_serde::BigInt<u64>")]
    pub pending_total_sui_withdraw: u64,
    /// Pending pool token withdrawn during the current epoch, emptied at epoch boundaries.
    #[serde_as(as = "sui_types::sui_serde::BigInt<u64>")]
    pub pending_pool_token_withdraw: u64,
    /// ID of the exchange rate table object.
    pub exchange_rates_id: ObjectId,
    /// Number of exchange rates in the table.
    #[serde_as(as = "sui_types::sui_serde::BigInt<u64>")]
    pub exchange_rates_size: u64,
}

impl From<sui_types::sui_system_state::sui_system_state_summary::SuiValidatorSummary>
    for ValidatorSummary
{
    fn from(
        value: sui_types::sui_system_state::sui_system_state_summary::SuiValidatorSummary,
    ) -> Self {
        let sui_types::sui_system_state::sui_system_state_summary::SuiValidatorSummary {
            sui_address,
            protocol_pubkey_bytes,
            network_pubkey_bytes,
            worker_pubkey_bytes,
            proof_of_possession_bytes,
            name,
            description,
            image_url,
            project_url,
            net_address,
            p2p_address,
            primary_address,
            worker_address,
            next_epoch_protocol_pubkey_bytes,
            next_epoch_proof_of_possession,
            next_epoch_network_pubkey_bytes,
            next_epoch_worker_pubkey_bytes,
            next_epoch_net_address,
            next_epoch_p2p_address,
            next_epoch_primary_address,
            next_epoch_worker_address,
            voting_power,
            operation_cap_id,
            gas_price,
            commission_rate,
            next_epoch_stake,
            next_epoch_gas_price,
            next_epoch_commission_rate,
            staking_pool_id,
            staking_pool_activation_epoch,
            staking_pool_deactivation_epoch,
            staking_pool_sui_balance,
            rewards_pool,
            pool_token_balance,
            pending_stake,
            pending_total_sui_withdraw,
            pending_pool_token_withdraw,
            exchange_rates_id,
            exchange_rates_size,
        } = value;

        Self {
            address: sui_address.into(),
            protocol_public_key: sui_sdk2::types::Bls12381PublicKey::from_bytes(
                protocol_pubkey_bytes,
            )
            .unwrap(),
            network_public_key: sui_sdk2::types::Ed25519PublicKey::from_bytes(network_pubkey_bytes)
                .unwrap(),
            worker_public_key: sui_sdk2::types::Ed25519PublicKey::from_bytes(worker_pubkey_bytes)
                .unwrap(),
            proof_of_possession_bytes,
            name,
            description,
            image_url,
            project_url,
            net_address,
            p2p_address,
            primary_address,
            worker_address,
            next_epoch_protocol_public_key: next_epoch_protocol_pubkey_bytes
                .map(|bytes| sui_sdk2::types::Bls12381PublicKey::from_bytes(bytes).unwrap()),
            next_epoch_network_public_key: next_epoch_network_pubkey_bytes
                .map(|bytes| sui_sdk2::types::Ed25519PublicKey::from_bytes(bytes).unwrap()),
            next_epoch_worker_public_key: next_epoch_worker_pubkey_bytes
                .map(|bytes| sui_sdk2::types::Ed25519PublicKey::from_bytes(bytes).unwrap()),
            next_epoch_proof_of_possession,
            next_epoch_net_address,
            next_epoch_p2p_address,
            next_epoch_primary_address,
            next_epoch_worker_address,
            voting_power,
            operation_cap_id: operation_cap_id.into(),
            gas_price,
            commission_rate,
            next_epoch_stake,
            next_epoch_gas_price,
            next_epoch_commission_rate,
            staking_pool_id: staking_pool_id.into(),
            staking_pool_activation_epoch,
            staking_pool_deactivation_epoch,
            staking_pool_sui_balance,
            rewards_pool,
            pool_token_balance,
            pending_stake,
            pending_total_sui_withdraw,
            pending_pool_token_withdraw,
            exchange_rates_id: exchange_rates_id.into(),
            exchange_rates_size,
        }
    }
}

impl From<sui_types::sui_system_state::sui_system_state_summary::SuiSystemStateSummary>
    for SystemStateSummary
{
    fn from(
        value: sui_types::sui_system_state::sui_system_state_summary::SuiSystemStateSummary,
    ) -> Self {
        let sui_types::sui_system_state::sui_system_state_summary::SuiSystemStateSummary {
            epoch,
            protocol_version,
            system_state_version,
            storage_fund_total_object_storage_rebates,
            storage_fund_non_refundable_balance,
            reference_gas_price,
            safe_mode,
            safe_mode_storage_rewards,
            safe_mode_computation_rewards,
            safe_mode_storage_rebates,
            safe_mode_non_refundable_storage_fee,
            epoch_start_timestamp_ms,
            epoch_duration_ms,
            stake_subsidy_start_epoch,
            max_validator_count,
            min_validator_joining_stake,
            validator_low_stake_threshold,
            validator_very_low_stake_threshold,
            validator_low_stake_grace_period,
            stake_subsidy_balance,
            stake_subsidy_distribution_counter,
            stake_subsidy_current_distribution_amount,
            stake_subsidy_period_length,
            stake_subsidy_decrease_rate,
            total_stake,
            active_validators,
            pending_active_validators_id,
            pending_active_validators_size,
            pending_removals,
            staking_pool_mappings_id,
            staking_pool_mappings_size,
            inactive_pools_id,
            inactive_pools_size,
            validator_candidates_id,
            validator_candidates_size,
            at_risk_validators,
            validator_report_records,
        } = value;

        Self {
            epoch,
            protocol_version,
            system_state_version,
            storage_fund_total_object_storage_rebates,
            storage_fund_non_refundable_balance,
            reference_gas_price,
            safe_mode,
            safe_mode_storage_rewards,
            safe_mode_computation_rewards,
            safe_mode_storage_rebates,
            safe_mode_non_refundable_storage_fee,
            epoch_start_timestamp_ms,
            epoch_duration_ms,
            stake_subsidy_start_epoch,
            max_validator_count,
            min_validator_joining_stake,
            validator_low_stake_threshold,
            validator_very_low_stake_threshold,
            validator_low_stake_grace_period,
            stake_subsidy_balance,
            stake_subsidy_distribution_counter,
            stake_subsidy_current_distribution_amount,
            stake_subsidy_period_length,
            stake_subsidy_decrease_rate,
            total_stake,
            active_validators: active_validators.into_iter().map(Into::into).collect(),
            pending_active_validators_id: pending_active_validators_id.into(),
            pending_active_validators_size,
            pending_removals,
            staking_pool_mappings_id: staking_pool_mappings_id.into(),
            staking_pool_mappings_size,
            inactive_pools_id: inactive_pools_id.into(),
            inactive_pools_size,
            validator_candidates_id: validator_candidates_id.into(),
            validator_candidates_size,
            at_risk_validators: at_risk_validators
                .into_iter()
                .map(|(address, idx)| (address.into(), idx))
                .collect(),
            validator_report_records: validator_report_records
                .into_iter()
                .map(|(address, reports)| {
                    (
                        address.into(),
                        reports.into_iter().map(Into::into).collect(),
                    )
                })
                .collect(),
        }
    }
}

pub struct GetCurrentProtocolConfig;

impl ApiEndpoint<RestService> for GetCurrentProtocolConfig {
    fn method(&self) -> axum::http::Method {
        axum::http::Method::GET
    }

    fn path(&self) -> &'static str {
        "/system/protocol"
    }

    fn operation(
        &self,
        _generator: &mut schemars::gen::SchemaGenerator,
    ) -> openapiv3::v3_1::Operation {
        openapiv3::v3_1::Operation::default()
    }

    fn handler(&self) -> RouteHandler<RestService> {
        RouteHandler::new(self.method(), get_current_protocol_config)
    }
}

async fn get_current_protocol_config(
    accept: AcceptFormat,
    State(state): State<StateReader>,
) -> Result<(SupportedProtocolHeaders, Json<ProtocolConfigResponse>)> {
    match accept {
        AcceptFormat::Json => {}
        _ => {
            return Err(RestError::new(
                axum::http::StatusCode::BAD_REQUEST,
                "invalid accept type",
            ))
        }
    }

    let current_protocol_version = state.get_system_state_summary()?.protocol_version;

    let config = ProtocolConfig::get_for_version_if_supported(
        current_protocol_version.into(),
        state.inner().get_chain_identifier()?.chain(),
    )
    .ok_or_else(|| ProtocolNotFoundError::new(current_protocol_version))?;

    Ok((supported_protocol_headers(), Json(config.into())))
}

pub struct GetProtocolConfig;

impl ApiEndpoint<RestService> for GetProtocolConfig {
    fn method(&self) -> axum::http::Method {
        axum::http::Method::GET
    }

    fn path(&self) -> &'static str {
        "/system/protocol/{version}"
    }

    fn operation(
        &self,
        _generator: &mut schemars::gen::SchemaGenerator,
    ) -> openapiv3::v3_1::Operation {
        openapiv3::v3_1::Operation::default()
    }

    fn handler(&self) -> RouteHandler<RestService> {
        RouteHandler::new(self.method(), get_protocol_config)
    }
}

async fn get_protocol_config(
    Path(version): Path<u64>,
    accept: AcceptFormat,
    State(state): State<StateReader>,
) -> Result<(SupportedProtocolHeaders, Json<ProtocolConfigResponse>)> {
    match accept {
        AcceptFormat::Json => {}
        _ => {
            return Err(RestError::new(
                axum::http::StatusCode::BAD_REQUEST,
                "invalid accept type",
            ))
        }
    }

    let config = ProtocolConfig::get_for_version_if_supported(
        version.into(),
        state.inner().get_chain_identifier()?.chain(),
    )
    .ok_or_else(|| ProtocolNotFoundError::new(version))?;

    Ok((supported_protocol_headers(), Json(config.into())))
}

#[derive(Debug)]
pub struct ProtocolNotFoundError {
    version: u64,
}

impl ProtocolNotFoundError {
    pub fn new(version: u64) -> Self {
        Self { version }
    }
}

impl std::fmt::Display for ProtocolNotFoundError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Protocol version {} not found", self.version)
    }
}

impl std::error::Error for ProtocolNotFoundError {}

impl From<ProtocolNotFoundError> for crate::RestError {
    fn from(value: ProtocolNotFoundError) -> Self {
        Self::new(axum::http::StatusCode::NOT_FOUND, value.to_string())
    }
}

/// Minimum supported protocol version by this node
pub const X_SUI_MIN_SUPPORTED_PROTOCOL_VERSION: &str = "x-sui-min-supported-protocol-version";
/// Maximum supported protocol version by this node
pub const X_SUI_MAX_SUPPORTED_PROTOCOL_VERSION: &str = "x-sui-max-supported-protocol-version";

type SupportedProtocolHeaders = [(&'static str, String); 2];

fn supported_protocol_headers() -> SupportedProtocolHeaders {
    [
        (
            X_SUI_MIN_SUPPORTED_PROTOCOL_VERSION,
            ProtocolVersion::MIN.as_u64().to_string(),
        ),
        (
            X_SUI_MAX_SUPPORTED_PROTOCOL_VERSION,
            ProtocolVersion::MAX.as_u64().to_string(),
        ),
    ]
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename = "ProtocolConfig")]
pub struct ProtocolConfigResponse {
    pub protocol_version: u64,
    pub feature_flags: BTreeMap<String, bool>,
    pub attributes: BTreeMap<String, String>,
}

impl From<ProtocolConfig> for ProtocolConfigResponse {
    fn from(config: ProtocolConfig) -> Self {
        let attributes = config
            .attr_map()
            .into_iter()
            .filter_map(|(k, maybe_v)| {
                maybe_v.map(move |v| {
                    let v = match v {
                        ProtocolConfigValue::u16(x) => x.to_string(),
                        ProtocolConfigValue::u32(y) => y.to_string(),
                        ProtocolConfigValue::u64(z) => z.to_string(),
                        ProtocolConfigValue::bool(b) => b.to_string(),
                    };
                    (k, v)
                })
            })
            .collect();
        ProtocolConfigResponse {
            protocol_version: config.version.as_u64(),
            attributes,
            feature_flags: config.feature_map(),
        }
    }
}

pub struct GetGasInfo;

impl ApiEndpoint<RestService> for GetGasInfo {
    fn method(&self) -> axum::http::Method {
        axum::http::Method::GET
    }

    fn path(&self) -> &'static str {
        "/system/gas"
    }

    fn handler(&self) -> RouteHandler<RestService> {
        RouteHandler::new(self.method(), get_gas_info)
    }
}

async fn get_gas_info(
    accept: AcceptFormat,
    State(state): State<StateReader>,
) -> Result<Json<GasInfo>> {
    match accept {
        AcceptFormat::Json => {}
        _ => {
            return Err(RestError::new(
                axum::http::StatusCode::BAD_REQUEST,
                "invalid accept type",
            ))
        }
    }

    let reference_gas_price = state.get_system_state_summary()?.reference_gas_price;

    Ok(Json(GasInfo {
        reference_gas_price,
    }))
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct GasInfo {
    #[serde(with = "serde_with::As::<serde_with::DisplayFromStr>")]
    reference_gas_price: u64,
}
