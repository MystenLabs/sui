// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::collections::HashMap;
use std::convert::Infallible;
use std::num::NonZeroUsize;
use std::sync::Mutex;

use anyhow::Context as _;
use diesel::ExpressionMethods;
use diesel::QueryDsl;
use futures::future;
use jsonrpsee::core::RpcResult;
use jsonrpsee::proc_macros::rpc;
use lru::LruCache;
use move_core_types::language_storage::StructTag;
use sui_indexer_alt_reader::consistent_reader::proto::owner::OwnerKind;
use sui_indexer_alt_reader::governance::RewardsKey;
use sui_indexer_alt_reader::governance::ValidatorAddressKey;
use sui_indexer_alt_schema::epochs::StoredEpochStart;
use sui_indexer_alt_schema::schema::kv_epoch_starts;
use sui_json_rpc_types::DelegatedStake;
use sui_json_rpc_types::Stake;
use sui_json_rpc_types::StakeStatus;
use sui_json_rpc_types::ValidatorApy;
use sui_json_rpc_types::ValidatorApys;
use sui_open_rpc::Module;
use sui_open_rpc_macros::open_rpc;
use sui_types::SUI_SYSTEM_ADDRESS;
use sui_types::SUI_SYSTEM_STATE_OBJECT_ID;
use sui_types::TypeTag;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SuiAddress;
use sui_types::dynamic_field::Field;
use sui_types::dynamic_field::derive_dynamic_field_id;
use sui_types::governance::STAKED_SUI_STRUCT_NAME;
use sui_types::governance::STAKING_POOL_MODULE_NAME;
use sui_types::governance::StakedSui;
use sui_types::sui_serde::BigInt;
use sui_types::sui_system_state::PoolTokenExchangeRate;
use sui_types::sui_system_state::SuiSystemState;
use sui_types::sui_system_state::SuiSystemStateTrait;
use sui_types::sui_system_state::SuiSystemStateWrapper;
use sui_types::sui_system_state::sui_system_state_inner_v1::SuiSystemStateInnerV1;
use sui_types::sui_system_state::sui_system_state_inner_v2::SuiSystemStateInnerV2;
use sui_types::sui_system_state::sui_system_state_summary::SuiSystemStateSummary;
use tokio::try_join;

use crate::api::rpc_module::RpcModule;
use crate::context::Context;
use crate::data::latest_epoch;
use crate::data::load_live;
use crate::data::load_live_deserialized;
use crate::error::RpcError;
use crate::error::rpc_bail;

/// Number of most recent epochs to load from `kv_epoch_starts` when computing validator APYs.
const APY_EPOCH_WINDOW: i64 = 31;

#[open_rpc(namespace = "suix", tag = "Governance API")]
#[rpc(server, namespace = "suix")]
trait GovernanceApi {
    /// Return the reference gas price for the network as of the latest epoch.
    #[method(name = "getReferenceGasPrice")]
    async fn get_reference_gas_price(&self) -> RpcResult<BigInt<u64>>;

    /// Return a summary of the latest version of the Sui System State object (0x5), on-chain.
    #[method(name = "getLatestSuiSystemState")]
    async fn get_latest_sui_system_state(&self) -> RpcResult<SuiSystemStateSummary>;

    /// Return one or more [DelegatedStake]. If a Stake was withdrawn its status will be Unstaked.
    #[method(name = "getStakesByIds")]
    async fn get_stakes_by_ids(
        &self,
        staked_sui_ids: Vec<ObjectID>,
    ) -> RpcResult<Vec<DelegatedStake>>;

    /// Return all [DelegatedStake].
    #[method(name = "getStakes")]
    async fn get_stakes(&self, owner: SuiAddress) -> RpcResult<Vec<DelegatedStake>>;

    /// Return the validator APY
    #[method(name = "getValidatorsApy")]
    async fn get_validators_apy(&self) -> RpcResult<ValidatorApys>;
}

pub(crate) struct Governance {
    ctx: Context,
    /// Caches the most recent `getValidatorsApy` response, keyed by epoch. APY inputs are fixed for
    /// the duration of an epoch, so a capacity-1 cache evicts automatically on epoch advance.
    apy_cache: Mutex<LruCache<u64, ValidatorApys>>,
}

impl Governance {
    pub fn new(ctx: Context) -> Self {
        Self {
            ctx,
            apy_cache: Mutex::new(LruCache::new(NonZeroUsize::new(1).unwrap())),
        }
    }
}

#[async_trait::async_trait]
impl GovernanceApiServer for Governance {
    async fn get_reference_gas_price(&self) -> RpcResult<BigInt<u64>> {
        Ok(rgp_response(&self.ctx).await?)
    }

    async fn get_latest_sui_system_state(&self) -> RpcResult<SuiSystemStateSummary> {
        Ok(latest_sui_system_state_response(&self.ctx).await?)
    }

    async fn get_stakes_by_ids(
        &self,
        staked_sui_ids: Vec<ObjectID>,
    ) -> RpcResult<Vec<DelegatedStake>> {
        Ok(delegated_stakes_response(&self.ctx, staked_sui_ids).await?)
    }

    async fn get_stakes(&self, owner: SuiAddress) -> RpcResult<Vec<DelegatedStake>> {
        let ctx = &self.ctx;
        let config = &ctx.config().objects;

        let tag = StructTag {
            address: SUI_SYSTEM_ADDRESS,
            module: STAKING_POOL_MODULE_NAME.to_owned(),
            name: STAKED_SUI_STRUCT_NAME.to_owned(),
            type_params: vec![],
        };

        let mut all_stake_ids: Vec<ObjectID> = Vec::new();
        let mut after_cursor = None;

        loop {
            let page = ctx
                .consistent_reader()
                .list_owned_objects(
                    None,
                    OwnerKind::Address,
                    Some(owner.to_string()),
                    Some(tag.to_canonical_string(true)),
                    Some(config.max_page_size as u32),
                    after_cursor,
                    None,
                    true,
                )
                .await
                .context("Failed to fetch owned StakedSui objects")
                .map_err(RpcError::<Infallible>::from)?;

            all_stake_ids.extend(page.results.iter().map(|edge| edge.value.0));

            if page.has_next_page {
                after_cursor = page.results.last().map(|edge| edge.token.clone());
            } else {
                break;
            }
        }

        Ok(delegated_stakes_response(ctx, all_stake_ids).await?)
    }

    async fn get_validators_apy(&self) -> RpcResult<ValidatorApys> {
        let ctx = &self.ctx;
        let epoch = latest_epoch(ctx)
            .await
            .context("Failed to fetch latest epoch for APY cache lookup")
            .map_err(RpcError::<Infallible>::from)?;

        if let Some(hit) = self.apy_cache.lock().unwrap().get(&epoch).cloned() {
            return Ok(hit);
        }

        let apys = validators_apy_response(ctx).await?;
        self.apy_cache.lock().unwrap().put(epoch, apys.clone());
        Ok(apys)
    }
}

impl RpcModule for Governance {
    fn schema(&self) -> Module {
        GovernanceApiOpenRpc::module_doc()
    }

    fn into_impl(self) -> jsonrpsee::RpcModule<Self> {
        self.into_rpc()
    }
}

/// Load data and generate response for `getReferenceGasPrice`.
async fn rgp_response(ctx: &Context) -> Result<BigInt<u64>, RpcError> {
    use kv_epoch_starts::dsl as e;

    let mut conn = ctx
        .pg_reader()
        .connect()
        .await
        .context("Failed to connect to the database")?;

    let rgp: i64 = conn
        .first(
            e::kv_epoch_starts
                .select(e::reference_gas_price)
                .order(e::epoch.desc()),
        )
        .await
        .context("Failed to fetch the reference gas price")?;

    Ok((rgp as u64).into())
}

/// Load data and generate response for `getLatestSuiSystemState`.
async fn latest_sui_system_state_response(
    ctx: &Context,
) -> Result<SuiSystemStateSummary, RpcError> {
    let wrapper: SuiSystemStateWrapper = load_live_deserialized(ctx, SUI_SYSTEM_STATE_OBJECT_ID)
        .await
        .context("Failed to fetch system state wrapper object")?;

    let inner_id = derive_dynamic_field_id(
        SUI_SYSTEM_STATE_OBJECT_ID,
        &TypeTag::U64,
        &bcs::to_bytes(&wrapper.version).context("Failed to serialize system state version")?,
    )
    .context("Failed to derive inner system state field ID")?;

    Ok(match wrapper.version {
        1 => load_live_deserialized::<Field<u64, SuiSystemStateInnerV1>>(ctx, inner_id)
            .await
            .context("Failed to fetch inner system state object")?
            .value
            .into_sui_system_state_summary(),
        2 => load_live_deserialized::<Field<u64, SuiSystemStateInnerV2>>(ctx, inner_id)
            .await
            .context("Failed to fetch inner system state object")?
            .value
            .into_sui_system_state_summary(),
        v => rpc_bail!("Unexpected inner system state version: {v}"),
    })
}

/// Given a list of StakedSui object IDs, load them, fetch rewards and validator addresses, and
/// return grouped DelegatedStake entries.
///
/// Returns only live staked objects. Stakes that have been withdrawn (or wrapped, deleted,
/// never existed) will be omitted from the response.
async fn delegated_stakes_response(
    ctx: &Context,
    stake_ids: Vec<ObjectID>,
) -> Result<Vec<DelegatedStake>, RpcError> {
    let execution_loader = ctx.execution_loader()?;

    let staked_sui_futures = stake_ids.iter().map(|id| load_live(ctx, *id));
    let maybe_objects = future::try_join_all(staked_sui_futures)
        .await
        .context("Failed to load StakedSui objects")?;

    let staked_suis: Vec<StakedSui> = maybe_objects
        .into_iter()
        .flatten()
        .map(|object| {
            let move_object = object.data.try_as_move().context("Not a Move object")?;
            bcs::from_bytes(move_object.contents()).context("Failed to deserialize StakedSui")
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    let reward_keys: Vec<RewardsKey> = staked_suis
        .iter()
        .map(|s| RewardsKey(s.id().into()))
        .collect();
    let validator_keys: Vec<ValidatorAddressKey> = staked_suis
        .iter()
        .map(|s| ValidatorAddressKey(s.pool_id().into()))
        .collect();

    let (rewards, validator_addresses, current_epoch) = try_join!(
        async {
            execution_loader
                .load_many(reward_keys)
                .await
                .context("Failed to dry run rewards calculation")
        },
        async {
            execution_loader
                .load_many(validator_keys)
                .await
                .context("Failed to dry run validator address lookup")
        },
        latest_epoch(ctx),
    )?;

    let mut grouped: BTreeMap<(SuiAddress, ObjectID), Vec<Stake>> = BTreeMap::new();

    // Clients can at most control which stake ids to query. Only live stakes are loaded. Valid
    // stakes should return a reward (could be 0 for pending stakes) and validator (pools are looked
    // up against active and inactive validators.)
    for staked_sui in &staked_suis {
        let reward_key = RewardsKey(staked_sui.id().into());
        let validator_key = ValidatorAddressKey(staked_sui.pool_id().into());

        let estimated_reward = *rewards
            .get(&reward_key)
            .with_context(|| format!("Missing reward for StakedSui {}", staked_sui.id()))?;
        let validator_address = validator_addresses
            .get(&validator_key)
            .map(|addr| SuiAddress::from(ObjectID::from(*addr)))
            .with_context(|| {
                format!(
                    "Missing validator address for staking pool {}",
                    staked_sui.pool_id()
                )
            })?;

        let status = if current_epoch >= staked_sui.activation_epoch() {
            StakeStatus::Active { estimated_reward }
        } else {
            StakeStatus::Pending
        };

        grouped
            .entry((validator_address, staked_sui.pool_id()))
            .or_default()
            .push(Stake {
                staked_sui_id: staked_sui.id(),
                stake_request_epoch: staked_sui.request_epoch(),
                stake_active_epoch: staked_sui.activation_epoch(),
                principal: staked_sui.principal(),
                status,
            });
    }

    Ok(grouped
        .into_iter()
        .map(
            |((validator_address, staking_pool), stakes)| DelegatedStake {
                validator_address,
                staking_pool,
                stakes,
            },
        )
        .collect())
}

/// Load data and generate response for `getValidatorsApy`.
///
/// Rates are derived from `staking_pool_sui_balance` and `pool_token_balance` of each active
/// validator in the latest `APY_EPOCH_WINDOW` rows of `kv_epoch_starts`. These are the same numbers
/// that `advance_epoch` writes into each staking pool's `exchange_rates` table. APYs are calculated
/// from adjacent pairs of rates, and then filtered and averaged to produce each validator's APY.
/// This mirrors the legacy fullnode jsonrpc's `backfill_rates` implementation.
///
/// In safe mode, a pool's sui and token balance carry over unchanged, which produces 0% APY and
/// also gets filtered out.
async fn validators_apy_response(ctx: &Context) -> Result<ValidatorApys, RpcError> {
    use kv_epoch_starts::dsl as e;

    let mut conn = ctx
        .pg_reader()
        .connect()
        .await
        .context("Failed to connect to the database")?;

    let rows: Vec<StoredEpochStart> = conn
        .results(
            e::kv_epoch_starts
                .order(e::epoch.desc())
                .limit(APY_EPOCH_WINDOW),
        )
        .await
        .context("Failed to fetch epoch starts for APY calculation")?;

    let latest = rows
        .first()
        .context("No epoch start rows available for APY calculation")?;

    let latest_summary = decode_summary(&latest.system_state)?;
    let current_epoch = latest_summary.epoch;
    let stake_subsidy_start_epoch = latest_summary.stake_subsidy_start_epoch;

    // Map pool to exchange rates history (newest to oldest)
    let mut by_pool: HashMap<ObjectID, Vec<PoolTokenExchangeRate>> = HashMap::new();
    for row in &rows {
        if (row.epoch as u64) < stake_subsidy_start_epoch {
            continue;
        }
        let summary = decode_summary(&row.system_state)?;
        for v in summary.active_validators {
            by_pool
                .entry(v.staking_pool_id)
                .or_default()
                .push(PoolTokenExchangeRate::new(
                    v.staking_pool_sui_balance,
                    v.pool_token_balance,
                ));
        }
    }

    let apys = latest_summary
        .active_validators
        .into_iter()
        .map(|v| ValidatorApy {
            address: v.sui_address,
            apy: by_pool
                .get(&v.staking_pool_id)
                .map_or(0.0, |rates| compute_apy(rates)),
        })
        .collect();

    Ok(ValidatorApys {
        apys,
        epoch: current_epoch,
    })
}

fn decode_summary(bytes: &[u8]) -> Result<SuiSystemStateSummary, RpcError> {
    Ok(bcs::from_bytes::<SuiSystemState>(bytes)
        .context("Failed to deserialize SuiSystemState from kv_epoch_starts")?
        .into_sui_system_state_summary())
}

/// Compute the average APY from a descending-epoch list of exchange rates.
///
/// Iterates adjacent pairs (newer, older), converts each pair into an annualized return
/// `(older / newer) ^ 365 - 1`, discards outliers outside `(0.0, 0.1)`, and averages up to 30 of
/// the remaining samples. This mirrors the legacy `calculate_apys` logic.
fn compute_apy(rates: &[PoolTokenExchangeRate]) -> f64 {
    let samples: Vec<f64> = rates
        .windows(2)
        .map(|w| (w[1].rate() / w[0].rate()).powf(365.0) - 1.0)
        .filter(|apy| *apy > 0.0 && *apy < 0.1)
        .take(30)
        .collect();

    if samples.is_empty() {
        0.0
    } else {
        samples.iter().sum::<f64>() / samples.len() as f64
    }
}
