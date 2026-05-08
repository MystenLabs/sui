// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `MergeAndRedeemFungibleStakedSui` construction logic.
//!
//! Translates a user's `(validator, amount, redeem_mode)` request into a PTB
//! that consumes the sender's `FungibleStakedSui` for the named validator's
//! pool and produces liquid SUI.
//!
//! The semantics differ by mode:
//!
//! * `All` — redeem every FSS the sender owns for the pool.
//! * `AtLeast(amount)` — choose the minimum pool-token count whose redemption
//!   yields at least `amount` MIST. The PTB also installs a runtime
//!   `balance::split` guard so a slight under-delivery aborts the transaction
//!   on chain instead of silently shorting the user.
//! * `AtMost(amount)` — choose the maximum pool-token count whose chain-side
//!   upper bound `expected = floor(token * sui_balance / pool_token_balance)`
//!   stays at or below `amount`. The pool's invariant
//!   `actual <= expected` then guarantees the cap holds at execution time.
//!   This is conservative: actual SUI may fall a few MIST short of the cap.
//!
//! Amount-sensitive plans (`AtLeast` / `AtMost`) bind the resulting transaction
//! to the quote epoch via `TransactionExpiration`, so a stale quote cannot
//! execute against a different epoch's exchange rate.

use async_trait::async_trait;
use futures::TryStreamExt;
use prost_types::FieldMask;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use sui_rpc::client::Client;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::{ListDynamicFieldsRequest, ListOwnedObjectsRequest};
use sui_types::base_types::{ObjectID, ObjectRef, SuiAddress};
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::rpc_proto_conversions::ObjectReferenceExt;
use sui_types::sui_system_state::SUI_SYSTEM_MODULE_NAME;
use sui_types::transaction::{CallArg, Command, ObjectArg, ProgrammableTransaction};
use sui_types::{Identifier, SUI_FRAMEWORK_PACKAGE_ID, SUI_SYSTEM_PACKAGE_ID};

use crate::account::FungibleStakedSuiBcs;
use crate::errors::Error;
use crate::types::{RedeemMode, RedeemPlan};

use super::{TransactionObjectData, TryConstructTransaction, simulate_transaction};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MergeAndRedeemFungibleStakedSui {
    pub sender: SuiAddress,
    pub validator: SuiAddress,
    pub amount: Option<u64>,
    pub redeem_mode: RedeemMode,
}

// ============================================================================
// Mirror of the on-chain exchange-rate and split-floor payout formulae
// (pure, no RPC).
// ============================================================================

/// Snapshot of the redeem-relevant state of a staking pool's
/// `FungibleStakedSuiData` plus the latest exchange rate, sufficient to mirror
/// the chain's payout formula in pure Rust.
#[derive(Clone, Debug)]
pub(crate) struct PoolRedeemData {
    /// Latest `exchange_rate.sui_amount` (= pool.sui_balance within an epoch).
    pub rate_sui: u64,
    /// Latest `exchange_rate.pool_token_amount` (= pool.pool_token_balance).
    pub rate_token: u64,
    /// `FungibleStakedSuiData.principal.value()` — current Balance<SUI> in the
    /// FSS sub-pool. Mutates within an epoch as other users convert/redeem.
    pub fss_principal: u64,
    /// `FungibleStakedSuiData.total_supply` — total outstanding FSS pool tokens.
    pub fss_total_supply: u64,
}

/// `floor(a * b / c)` with checked u64 overflow. Move's `mul_div!` aborts on
/// overflow; this helper returns `Err(DataError)` to match — silently truncating
/// to `u64` would let the off-chain mirror disagree with the chain in pathological
/// rate scenarios.
fn mul_div_u64(a: u64, b: u64, c: u64) -> Result<u64, Error> {
    if c == 0 {
        return Err(Error::DataError("mul_div_u64: divide by zero".into()));
    }
    let v = (a as u128) * (b as u128) / (c as u128);
    u64::try_from(v).map_err(|_| {
        Error::DataError(format!(
            "redeem math overflow: floor({a} * {b} / {c}) does not fit in u64"
        ))
    })
}

/// Mirror of `0x3::staking_pool::PoolTokenExchangeRate::get_sui_amount` at
/// `staking_pool.move:638-646`. Returns the SUI cap for `token_amount` pool
/// tokens given an exchange rate of `(rate_sui, rate_token)`.
///
/// The chain enforces the invariant
/// `actual_redeemed <= floor(token * rate_sui / rate_token)` even after the
/// per-redeemer split-floor accounting in `calculate_fungible_staked_sui_withdraw_amount`
/// (see `staking_pool.move:264-268`), so any token count satisfying
/// `expected_sui_amount(token) <= max_sui` will stay within the cap at
/// execution time regardless of intra-epoch pool drift.
pub(crate) fn expected_sui_amount(
    rate_sui: u64,
    rate_token: u64,
    token_amount: u64,
) -> Result<u64, Error> {
    if rate_sui == 0 || rate_token == 0 {
        return Ok(token_amount);
    }
    mul_div_u64(token_amount, rate_sui, rate_token)
}

/// Mirror of `0x3::staking_pool::calculate_fungible_staked_sui_withdraw_amount`
/// at `staking_pool.move:231-271`. Returns the actual SUI the chain will
/// deliver for `token_amount` FSS pool tokens given the supplied pool state.
///
/// The chain splits the payout into a principal share and a rewards share, each
/// with an independent `floor`, so `actual <= expected` and the gap is bounded
/// by ~2 MIST per redemption (one MIST per floor, scaled by `token/supply`).
/// AtLeast selection must use this (not `expected_sui_amount`) to avoid picking
/// a token count whose actual payout falls below the user's `min_sui`.
///
/// Returns `Err(DataError)` if any intermediate `mul_div` overflows u64 — the
/// chain's `mul_div!` aborts on overflow, so the mirror surfaces the same
/// failure mode rather than silently truncating.
pub(crate) fn mirror_redeem_actual(data: &PoolRedeemData, token_amount: u64) -> Result<u64, Error> {
    if data.fss_total_supply == 0 {
        return Ok(0);
    }

    // total_sui = exchange_rate.get_sui_amount(total_supply)
    let total_sui = if data.rate_sui == 0 || data.rate_token == 0 {
        data.fss_total_supply
    } else {
        mul_div_u64(data.rate_sui, data.fss_total_supply, data.rate_token)?
    };

    let principal = data.fss_principal.min(total_sui);
    let rewards = total_sui - principal;

    let principal_out = mul_div_u64(token_amount, principal, data.fss_total_supply)?;
    let rewards_out = mul_div_u64(token_amount, rewards, data.fss_total_supply)?;

    principal_out
        .checked_add(rewards_out)
        .ok_or_else(|| Error::DataError("redeem math overflow: principal_out + rewards_out".into()))
}

/// Binary-search the smallest `token_amount` in `[1, total_tokens]` such that
/// `mirror_redeem_actual(data, token_amount) >= target_sui`.
///
/// Returns `Ok(None)` if even redeeming `total_tokens` produces less than
/// `target_sui` (caller reports "Insufficient FSS balance"). Returns
/// `Err` only if the underlying mirror math would overflow u64.
pub(crate) fn binary_search_at_least(
    data: &PoolRedeemData,
    total_tokens: u64,
    target_sui: u64,
) -> Result<Option<u64>, Error> {
    if total_tokens == 0 {
        return Ok(None);
    }
    if mirror_redeem_actual(data, total_tokens)? < target_sui {
        return Ok(None);
    }
    let mut lo: u64 = 1;
    let mut hi: u64 = total_tokens;
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if mirror_redeem_actual(data, mid)? >= target_sui {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    Ok(Some(lo))
}

/// Binary-search the largest `token_amount` in `[1, total_tokens]` such that
/// `expected_sui_amount(rate_sui, rate_token, token_amount) <= max_sui`.
///
/// Returns `Ok(None)` if even one token would already exceed `max_sui`
/// (caller reports "AtMost amount too small to redeem any tokens"). Returns
/// `Err` only if the underlying mirror math would overflow u64.
pub(crate) fn binary_search_at_most(
    rate_sui: u64,
    rate_token: u64,
    total_tokens: u64,
    max_sui: u64,
) -> Result<Option<u64>, Error> {
    if total_tokens == 0 {
        return Ok(None);
    }
    if expected_sui_amount(rate_sui, rate_token, 1)? > max_sui {
        return Ok(None);
    }
    let mut lo: u64 = 1;
    let mut hi: u64 = total_tokens;
    let mut ans: Option<u64> = None;
    while lo <= hi {
        let mid = lo + (hi - lo) / 2;
        if expected_sui_amount(rate_sui, rate_token, mid)? <= max_sui {
            ans = Some(mid);
            if mid == u64::MAX {
                break;
            }
            lo = mid + 1;
        } else {
            if mid == 0 {
                break;
            }
            hi = mid - 1;
        }
    }
    Ok(ans)
}

// ============================================================================
// Pool / FSS resolution (FSS-first, supports inactive validators).
// ============================================================================

/// Information extracted from a single owned `FungibleStakedSui` object.
#[derive(Clone, Debug)]
struct OwnedFss {
    object_ref: ObjectRef,
    pool_id: ObjectID,
    value: u64,
}

async fn list_owned_fss(client: &mut Client, sender: SuiAddress) -> Result<Vec<OwnedFss>, Error> {
    let request = ListOwnedObjectsRequest::default()
        .with_owner(sender.to_string())
        .with_object_type("0x3::staking_pool::FungibleStakedSui".to_string())
        .with_page_size(1000u32)
        .with_read_mask(FieldMask::from_paths([
            "object_id",
            "version",
            "digest",
            "contents",
        ]));
    let objects: Vec<_> = client
        .list_owned_objects(request)
        .map_err(Error::from)
        .try_collect()
        .await?;

    let mut out = Vec::with_capacity(objects.len());
    for obj in &objects {
        let contents = obj
            .contents
            .as_ref()
            .ok_or_else(|| Error::DataError("FungibleStakedSui missing contents".to_string()))?;
        let fss: FungibleStakedSuiBcs = contents.deserialize().map_err(|e| {
            Error::DataError(format!("Failed to deserialize FungibleStakedSui: {e}"))
        })?;
        let pool_id = ObjectID::from_str(&fss.pool_id.to_string())
            .map_err(|e| Error::DataError(format!("Invalid pool_id: {e}")))?;
        let object_id = ObjectID::from_str(obj.object_id())
            .map_err(|e| Error::DataError(format!("Invalid object_id: {e}")))?;
        let object_ref: ObjectRef = (
            object_id,
            obj.version().into(),
            obj.digest()
                .parse()
                .map_err(|e| Error::DataError(format!("Invalid digest: {e}")))?,
        );
        out.push(OwnedFss {
            object_ref,
            pool_id,
            value: fss.value,
        });
    }
    Ok(out)
}

/// Resolve `validator` to a pool, the sender's FSS in that pool, and the
/// `(rate_sui, rate_token, epoch)` snapshot. Supports both active and inactive
/// validator pools — the chain's `validator_set::redeem_fungible_staked_sui`
/// (validator_set.move:346-364) routes through `staking_pool_mappings` for
/// active pools and falls back to `inactive_validators[pool_id]` otherwise,
/// so Rosetta must accept the same set.
///
/// FSS-first: enumerate sender's FSS, group by pool_id, then for each candidate
/// pool look up the validator address (active fast path, inactive_validators
/// dynamic field walk fallback). The first pool whose validator address matches
/// `target_validator` is selected.
///
/// **Atomicity is load-bearing**: amount-sensitive plans bind the resulting
/// transaction to `epoch`, so the rate must come from the same RPC response
/// as the inactive-table lookup. Otherwise an epoch transition between the
/// rate read and the bind_epoch read could leave the quote pinned to a stale
/// rate, silently violating AtMost caps and aborting AtLeast guards.
async fn resolve_pool_fss_and_rate(
    client: &mut Client,
    sender: SuiAddress,
    target_validator: SuiAddress,
) -> Result<PoolFssRateSnapshot, Error> {
    let owned = list_owned_fss(client, sender).await?;
    if owned.is_empty() {
        return Err(Error::InvalidInput(format!(
            "No FungibleStakedSui found for sender {sender}"
        )));
    }

    let snapshot = crate::account::get_validator_set_snapshot(client).await?;
    let target_validator_addr = sui_sdk_types::Address::from(target_validator);

    // Group FSS by pool_id; deterministic ordering (BTreeMap on ObjectID).
    let mut by_pool: std::collections::BTreeMap<ObjectID, Vec<&OwnedFss>> =
        std::collections::BTreeMap::new();
    for fss in &owned {
        by_pool.entry(fss.pool_id).or_default().push(fss);
    }

    for (pool_id, fss_list) in &by_pool {
        // Active fast path.
        if let Some(rate_info) = snapshot.active_rates.get(&pool_id.to_string())
            && rate_info.validator_address == target_validator_addr
        {
            return Ok(build_snapshot(fss_list, rate_info, snapshot.epoch));
        }

        // Inactive fallback.
        if let Some(table_id_str) = snapshot.inactive_validators_table_id.as_deref() {
            let table_id = ObjectID::from_str(table_id_str).map_err(|e| {
                Error::DataError(format!("Invalid inactive_validators table id: {e}"))
            })?;
            if let Some(inactive) =
                lookup_inactive_pool(client, table_id, *pool_id, target_validator).await?
            {
                let (rate_sui, rate_token) = fetch_pool_exchange_rate_at_epoch(
                    client,
                    inactive.exchange_rates_table_id,
                    inactive.activation_epoch,
                    inactive.deactivation_epoch,
                    snapshot.epoch,
                )
                .await?;
                let mut sorted = fss_list.clone();
                sorted.sort_by_key(|f| f.object_ref.0);
                let total_tokens: u64 = sorted.iter().map(|f| f.value).sum();
                let fss_refs: Vec<ObjectRef> = sorted.iter().map(|f| f.object_ref).collect();
                return Ok(PoolFssRateSnapshot {
                    fss_refs,
                    total_tokens,
                    rate_sui,
                    rate_token,
                    epoch: snapshot.epoch,
                    pool_extra_fields_id: Some(inactive.pool_extra_fields_id),
                });
            }
        }
    }

    Err(Error::InvalidInput(format!(
        "No FungibleStakedSui found for validator {target_validator}'s pool \
         (sender holds {} FSS object(s) but none are for that validator's active \
          or inactive pool)",
        owned.len()
    )))
}

fn build_snapshot(
    fss_list: &[&OwnedFss],
    rate_info: &crate::account::PoolRateInfo,
    epoch: u64,
) -> PoolFssRateSnapshot {
    let mut sorted = fss_list.to_vec();
    sorted.sort_by_key(|f| f.object_ref.0);
    let total_tokens: u64 = sorted.iter().map(|f| f.value).sum();
    let fss_refs: Vec<ObjectRef> = sorted.iter().map(|f| f.object_ref).collect();
    let pool_extra_fields_id = rate_info
        .pool_extra_fields_id
        .as_deref()
        .and_then(|s| ObjectID::from_str(s).ok());
    PoolFssRateSnapshot {
        fss_refs,
        total_tokens,
        rate_sui: rate_info.sui_balance,
        rate_token: rate_info.pool_token_balance,
        epoch,
        pool_extra_fields_id,
    }
}

/// Pool data extracted from an inactive validator's `ValidatorWrapper`.
///
/// The on-chain redeem path (`staking_pool.move:205`) calls
/// `pool.pool_token_exchange_rate_at_epoch(ctx.epoch())` rather than reading
/// `pool.sui_balance / pool_token_balance` directly. For inactive pools these
/// live fields can drift from the exchange-rate snapshot when other users do
/// regular `request_withdraw_stake` (StakedSui, not FSS) on the deactivated
/// pool — `staking_pool.move:187-188` immediately processes the pending
/// withdrawal for inactive/preactive pools, which mutates `sui_balance` and
/// `pool_token_balance`. So we extract the table id and walk it the same way
/// the chain does.
struct InactivePoolData {
    activation_epoch: Option<u64>,
    deactivation_epoch: Option<u64>,
    exchange_rates_table_id: ObjectID,
    pool_extra_fields_id: ObjectID,
}

/// Walk `inactive_validators[pool_id]` → `ValidatorWrapper` → `Versioned` →
/// `ValidatorV1` to find the validator with matching address and extract the
/// pool's exchange-rate table id (plus activation/deactivation epochs needed
/// for the walk-back) and the FSS data Bag id.
///
/// The chain stores `inactive_validators` as `Table<ID, ValidatorWrapper>`
/// (validator_set.move:73). Each entry is a dynamic field at
/// `derive(table.id, &TypeTag::ID, bcs(pool_id))`. The wrapper holds a
/// `Versioned` whose latest value is the actual `ValidatorV1`, stored at
/// `derive(versioned.id, &TypeTag::U64, bcs(version))`.
///
/// Returns `None` if the wrapper at the derived id resolves to a validator
/// whose address doesn't match (caller can try the next candidate pool).
async fn lookup_inactive_pool(
    client: &mut Client,
    inactive_table_id: ObjectID,
    pool_id: ObjectID,
    expected_validator: SuiAddress,
) -> Result<Option<InactivePoolData>, Error> {
    use sui_types::dynamic_field::{Field, derive_dynamic_field_id};
    use sui_types::id::ID;
    use sui_types::sui_system_state::ValidatorWrapper;
    use sui_types::sui_system_state::sui_system_state_inner_v1::ValidatorV1;

    let id_type = sui_types::TypeTag::from_str("0x2::object::ID")
        .map_err(|e| Error::DataError(format!("ID TypeTag: {e}")))?;
    let pool_id_bcs =
        bcs::to_bytes(&pool_id).map_err(|e| Error::DataError(format!("pool_id bcs: {e}")))?;
    let wrapper_field_id = derive_dynamic_field_id(inactive_table_id, &id_type, &pool_id_bcs)
        .map_err(|e| Error::DataError(format!("derive inactive wrapper field id: {e}")))?;

    let Some(wrapper_bytes) = try_read_object_contents(client, wrapper_field_id).await? else {
        return Ok(None);
    };
    let wrapper_field: Field<ID, ValidatorWrapper> = bcs::from_bytes(&wrapper_bytes)
        .map_err(|e| Error::DataError(format!("Field<ID,ValidatorWrapper> decode: {e}")))?;
    let versioned_id = wrapper_field.value.inner.id.id.bytes;
    let version = wrapper_field.value.inner.version;

    let u64_type = sui_types::TypeTag::U64;
    let version_bcs =
        bcs::to_bytes(&version).map_err(|e| Error::DataError(format!("version bcs: {e}")))?;
    let validator_field_id = derive_dynamic_field_id(versioned_id, &u64_type, &version_bcs)
        .map_err(|e| Error::DataError(format!("derive ValidatorV1 field id: {e}")))?;

    let validator_bytes = try_read_object_contents(client, validator_field_id)
        .await?
        .ok_or_else(|| {
            Error::DataError(format!(
                "Inactive validator wrapper at {wrapper_field_id} points at \
                 missing ValidatorV1 at {validator_field_id} (version {version})"
            ))
        })?;
    let validator_field: Field<u64, ValidatorV1> = bcs::from_bytes(&validator_bytes)
        .map_err(|e| Error::DataError(format!("Field<u64,ValidatorV1> decode: {e}")))?;
    let validator = validator_field.value;
    if validator.metadata.sui_address != expected_validator {
        return Ok(None);
    }

    Ok(Some(InactivePoolData {
        activation_epoch: validator.staking_pool.activation_epoch,
        deactivation_epoch: validator.staking_pool.deactivation_epoch,
        exchange_rates_table_id: validator.staking_pool.exchange_rates.id,
        pool_extra_fields_id: validator.staking_pool.extra_fields.id.id.bytes,
    }))
}

/// Mirror of `staking_pool::pool_token_exchange_rate_at_epoch`
/// (`staking_pool.move:587-608`). Returns `(sui_amount, pool_token_amount)`
/// of the exchange rate the chain would use when redeeming at `current_epoch`.
///
/// For deactivated pools the lookup epoch is clamped to `deactivation_epoch`
/// (rates aren't recorded after deactivation; the last recorded rate is the
/// one the chain replays). The walk steps backward from the clamped epoch
/// down to `activation_epoch` and returns the first `exchange_rates[epoch]`
/// entry it finds. If none is found (or the pool is preactive) the chain
/// returns `initial_exchange_rate()` which `get_sui_amount` then treats as
/// 1:1 — we surface the same `(0, 0)` here so downstream `expected_sui_amount`
/// applies the same fallback.
async fn fetch_pool_exchange_rate_at_epoch(
    client: &mut Client,
    table_id: ObjectID,
    activation_epoch: Option<u64>,
    deactivation_epoch: Option<u64>,
    current_epoch: u64,
) -> Result<(u64, u64), Error> {
    let candidates = walk_back_epochs(activation_epoch, deactivation_epoch, current_epoch);
    for epoch in candidates {
        if let Some((sui_amount, pool_token_amount)) =
            try_read_exchange_rate(client, table_id, epoch).await?
        {
            return Ok((sui_amount, pool_token_amount));
        }
    }
    Ok((0, 0))
}

/// Pure-function half of the walk-back: given the pool's activation /
/// deactivation epochs and the current epoch, produce the ordered list of
/// epochs to probe, matching `pool_token_exchange_rate_at_epoch` semantics.
///
/// * Preactive pool (activation > current, or never activated) → empty list:
///   the on-chain `is_preactive_at_epoch` short-circuits to
///   `initial_exchange_rate()`.
/// * Otherwise clamp to `min(deactivation.unwrap_or(current), current)` and
///   walk down to `activation`.
fn walk_back_epochs(
    activation_epoch: Option<u64>,
    deactivation_epoch: Option<u64>,
    current_epoch: u64,
) -> Vec<u64> {
    let Some(activation) = activation_epoch else {
        return Vec::new();
    };
    if activation > current_epoch {
        return Vec::new();
    }
    let clamped = deactivation_epoch
        .map(|d| d.min(current_epoch))
        .unwrap_or(current_epoch);
    let start = clamped.max(activation);
    (activation..=start).rev().collect()
}

/// BCS layout of `0x3::staking_pool::PoolTokenExchangeRate`. Mirrors the Move
/// struct at `staking_pool.move:69-72`. We can't use `sui_types`'s
/// `PoolTokenExchangeRate` directly because its fields are private; the BCS
/// layout (positional u64 / u64) is the public interface.
#[derive(Deserialize, Debug)]
struct PoolTokenExchangeRateBcs {
    sui_amount: u64,
    pool_token_amount: u64,
}

/// Direct dynamic-field lookup of `exchange_rates[epoch]`. Returns
/// `Ok(None)` if the entry doesn't exist at the derived id (no rate was
/// recorded for that epoch — caller walks back to the previous one).
async fn try_read_exchange_rate(
    client: &mut Client,
    table_id: ObjectID,
    epoch: u64,
) -> Result<Option<(u64, u64)>, Error> {
    use sui_types::dynamic_field::{Field, derive_dynamic_field_id};

    let key_bcs = bcs::to_bytes(&epoch)
        .map_err(|e| Error::DataError(format!("exchange_rates key bcs: {e}")))?;
    let field_id = derive_dynamic_field_id(table_id, &sui_types::TypeTag::U64, &key_bcs)
        .map_err(|e| Error::DataError(format!("derive exchange_rates[{epoch}] field id: {e}")))?;

    let Some(bytes) = try_read_object_contents(client, field_id).await? else {
        return Ok(None);
    };
    let field: Field<u64, PoolTokenExchangeRateBcs> = bcs::from_bytes(&bytes).map_err(|e| {
        Error::DataError(format!(
            "Field<u64,PoolTokenExchangeRate> decode at epoch {epoch}: {e}"
        ))
    })?;
    Ok(Some((
        field.value.sui_amount,
        field.value.pool_token_amount,
    )))
}

/// Fetch raw BCS contents of an object, returning `None` if the object does
/// not exist (so callers can probe derived addresses without erroring).
async fn try_read_object_contents(
    client: &mut Client,
    object_id: ObjectID,
) -> Result<Option<Vec<u8>>, Error> {
    let request = sui_rpc::proto::sui::rpc::v2::GetObjectRequest::default()
        .with_object_id(object_id.to_string())
        .with_read_mask(FieldMask::from_paths(["contents"]));
    let response = match client.ledger_client().get_object(request).await {
        Ok(r) => r.into_inner(),
        Err(status) if status.code() == tonic::Code::NotFound => return Ok(None),
        Err(e) => return Err(Error::from(e)),
    };
    let Some(obj) = response.object else {
        return Ok(None);
    };
    let bytes = obj
        .contents
        .as_ref()
        .and_then(|b| b.value.as_deref())
        .map(|v| v.to_vec())
        .unwrap_or_default();
    if bytes.is_empty() {
        Ok(None)
    } else {
        Ok(Some(bytes))
    }
}

/// Atomic snapshot tying together pool, FSS, exchange rate, and the epoch the
/// rate was read in. The epoch is the same one the resulting transaction will
/// be bound to via `TransactionExpiration`.
struct PoolFssRateSnapshot {
    fss_refs: Vec<ObjectRef>,
    total_tokens: u64,
    rate_sui: u64,
    rate_token: u64,
    epoch: u64,
    /// `pool.extra_fields.id` (the Bag UID), needed to fetch
    /// `FungibleStakedSuiData` for AtLeast plans.
    pool_extra_fields_id: Option<ObjectID>,
}

/// Subset of `0x3::staking_pool::FungibleStakedSuiData` we BCS-deserialize.
/// Field order MUST match the Move source at `staking_pool.move:98-104`:
///   `id: UID, total_supply: u64, principal: Balance<SUI>`
/// where `UID` is a 32-byte address and `Balance<SUI>` is a `{ value: u64 }`
/// struct (stored positionally as a u64).
#[derive(Deserialize, Debug)]
struct FungibleStakedSuiDataBcs {
    _id: sui_sdk_types::Address,
    total_supply: u64,
    principal: BalanceValueBcs,
}

#[derive(Deserialize, Debug)]
struct BalanceValueBcs {
    value: u64,
}

/// Fetch the `FungibleStakedSuiData` stored at
/// `pool.extra_fields[FungibleStakedSuiDataKey {}]`. Lists the bag's dynamic
/// fields and finds the one whose `value_type` matches
/// `<SUI_SYSTEM_PACKAGE>::staking_pool::FungibleStakedSuiData`, then BCS-
/// deserializes the value into `(principal_value, total_supply)`.
///
/// We list rather than derive the field id locally to avoid coupling to the
/// dynamic-field hashing rules (which would need a faithful Move-side
/// `bcs::to_bytes(&FungibleStakedSuiDataKey {})` and TypeTag canonicalization).
/// The bag normally holds a small number of entries (currently just FSS data),
/// so the cost is bounded.
async fn fetch_fss_data(client: &mut Client, bag_id: ObjectID) -> Result<(u64, u64), Error> {
    let want_type = format!(
        "{}::staking_pool::FungibleStakedSuiData",
        SUI_SYSTEM_PACKAGE_ID
    );

    let request = ListDynamicFieldsRequest::default()
        .with_parent(bag_id.to_string())
        .with_page_size(50u32)
        .with_read_mask(FieldMask::from_paths(["value_type", "value"]));
    let dynamic_fields: Vec<_> = client
        .list_dynamic_fields(request)
        .map_err(Error::from)
        .try_collect()
        .await?;

    for df in dynamic_fields {
        let Some(value_type) = df.value_type.as_deref() else {
            continue;
        };
        if !type_tags_match(value_type, &want_type) {
            continue;
        }
        let bytes = df
            .value
            .as_ref()
            .and_then(|b| b.value.as_deref())
            .ok_or_else(|| {
                Error::DataError("FungibleStakedSuiData entry has empty value".to_string())
            })?;
        let data: FungibleStakedSuiDataBcs = bcs::from_bytes(bytes)
            .map_err(|e| Error::DataError(format!("FungibleStakedSuiData decode: {e}")))?;
        return Ok((data.principal.value, data.total_supply));
    }

    Err(Error::DataError(format!(
        "No FungibleStakedSuiData entry found in pool extra_fields bag {bag_id} \
         (validator has no FSS yet — but sender holds FSS, this should not happen)"
    )))
}

/// Compare two Move type tag strings ignoring address normalization differences
/// (`0x3::...` vs `0x0000...3::...`). Both are valid canonical forms.
fn type_tags_match(a: &str, b: &str) -> bool {
    use sui_types::TypeTag;
    match (TypeTag::from_str(a), TypeTag::from_str(b)) {
        (Ok(ta), Ok(tb)) => ta == tb,
        _ => a == b,
    }
}

#[async_trait]
impl TryConstructTransaction for MergeAndRedeemFungibleStakedSui {
    async fn try_fetch_needed_objects(
        self,
        client: &mut Client,
        gas_price: Option<u64>,
        budget: Option<u64>,
    ) -> Result<TransactionObjectData, Error> {
        let Self {
            sender,
            validator,
            amount,
            redeem_mode,
        } = self;

        let snapshot = resolve_pool_fss_and_rate(client, sender, validator).await?;
        let PoolFssRateSnapshot {
            fss_refs,
            total_tokens,
            rate_sui,
            rate_token,
            epoch,
            pool_extra_fields_id,
        } = snapshot;

        // AtLeast needs the FSS sub-pool's principal+total_supply to mirror the
        // chain's split-floor payout formula and pick the minimum token count
        // whose actual payout >= min_sui. Fetched lazily so All/AtMost paths
        // skip the extra GetObject.
        let fss_data = if matches!(redeem_mode, RedeemMode::AtLeast) {
            let bag_id = pool_extra_fields_id.ok_or_else(|| {
                Error::DataError(
                    "Staking pool extra_fields.id missing from system state response — \
                     cannot locate FungibleStakedSuiData for AtLeast mode"
                        .to_string(),
                )
            })?;
            let (principal, total_supply) = fetch_fss_data(client, bag_id).await?;
            Some(PoolRedeemData {
                rate_sui,
                rate_token,
                fss_principal: principal,
                fss_total_supply: total_supply,
            })
        } else {
            None
        };

        let plan = build_redeem_plan(
            redeem_mode,
            amount,
            total_tokens,
            rate_sui,
            rate_token,
            fss_data.as_ref(),
        )?;
        // Amount-sensitive plans bind to `epoch` so the chain executes against
        // the same exchange rate the off-chain quote used. `epoch` here comes
        // from the same RPC response as `(rate_sui, rate_token)`, so there is
        // no race between rate read and epoch read.
        let bind_epoch = match plan {
            RedeemPlan::All => None,
            RedeemPlan::AtLeast { .. } | RedeemPlan::AtMost { .. } => Some(epoch),
        };

        let pt = merge_and_redeem_fss_pt(sender, fss_refs.clone(), &plan)?;
        let (budget, gas_coin_objs) =
            simulate_transaction(client, pt, sender, vec![], gas_price, budget).await?;

        let total_sui_balance = gas_coin_objs.iter().map(|c| c.balance()).sum::<u64>() as i128;
        let gas_coins = gas_coin_objs
            .iter()
            .map(|obj| obj.object_reference().try_to_object_ref())
            .collect::<Result<Vec<_>, _>>()?;

        let redeem_token_amount = match plan {
            RedeemPlan::All => None,
            RedeemPlan::AtLeast { token_amount, .. } | RedeemPlan::AtMost { token_amount, .. } => {
                token_amount
            }
        };

        Ok(TransactionObjectData {
            gas_coins,
            objects: fss_refs,
            party_objects: vec![],
            total_sui_balance,
            budget,
            address_balance_withdrawal: 0,
            fss_object_count: None,
            redeem_token_amount,
            redeem_plan: Some(plan),
            bind_epoch,
        })
    }
}

/// Translate a user-supplied `(redeem_mode, amount)` pair into a `RedeemPlan`.
///
/// `AtLeast` mirrors the chain's split-floor payout formula
/// (`calculate_fungible_staked_sui_withdraw_amount`) and binary-searches for
/// the minimum token count whose actual payout is `>= min_sui`. `fss_data`
/// must be `Some` for AtLeast — without it we'd fall back to the `expected`
/// formula which can pick a token count whose actual falls 1-2 MIST short,
/// causing the chain guard to abort needlessly.
///
/// `AtMost` searches by `expected` alone — the chain's `actual <= expected`
/// invariant (`staking_pool.move:264-268`) keeps the cap safe without the FSS
/// sub-pool data.
pub(crate) fn build_redeem_plan(
    redeem_mode: RedeemMode,
    amount: Option<u64>,
    total_tokens: u64,
    rate_sui: u64,
    rate_token: u64,
    fss_data: Option<&PoolRedeemData>,
) -> Result<RedeemPlan, Error> {
    match redeem_mode {
        RedeemMode::All => Ok(RedeemPlan::All),
        RedeemMode::AtLeast => {
            let min_sui = amount.ok_or_else(|| {
                Error::InvalidInput("amount required for AtLeast mode".to_string())
            })?;
            if min_sui == 0 {
                return Err(Error::InvalidInput(
                    "AtLeast amount must be at least 1 MIST".to_string(),
                ));
            }
            if rate_sui == 0 || rate_token == 0 {
                return Err(Error::DataError(
                    "Pool has zero exchange rate, cannot compute redeem plan".to_string(),
                ));
            }
            let data = fss_data.ok_or_else(|| {
                Error::DataError(
                    "FungibleStakedSuiData required for AtLeast mode but not supplied".to_string(),
                )
            })?;
            let token_amount =
                binary_search_at_least(data, total_tokens, min_sui)?.ok_or_else(|| {
                    Error::InvalidInput(format!(
                        "Insufficient FSS balance: cannot deliver AtLeast {min_sui} SUI \
                         from {total_tokens} pool tokens at current exchange rate",
                    ))
                })?;
            Ok(RedeemPlan::AtLeast {
                token_amount: full_redeem_or_split(token_amount, total_tokens),
                min_sui,
            })
        }
        RedeemMode::AtMost => {
            let max_sui = amount.ok_or_else(|| {
                Error::InvalidInput("amount required for AtMost mode".to_string())
            })?;
            if max_sui == 0 {
                return Err(Error::InvalidInput(
                    "AtMost amount must be at least 1 MIST".to_string(),
                ));
            }
            if rate_sui == 0 || rate_token == 0 {
                return Err(Error::DataError(
                    "Pool has zero exchange rate, cannot compute redeem plan".to_string(),
                ));
            }
            let token_amount = binary_search_at_most(rate_sui, rate_token, total_tokens, max_sui)?
                .ok_or_else(|| {
                    Error::InvalidInput(
                    "AtMost amount too small: would redeem 0 pool tokens at current exchange rate"
                        .to_string(),
                )
                })?;
            Ok(RedeemPlan::AtMost {
                token_amount: full_redeem_or_split(token_amount, total_tokens),
                max_sui,
            })
        }
    }
}

/// Decide whether a `token_amount` selected by binary search needs a
/// `split_fungible_staked_sui` step. If the search picked exactly the total,
/// the merged FSS is already the right size — splitting would just leave a
/// zero-value FSS object as dust.
fn full_redeem_or_split(token_amount: u64, total_tokens: u64) -> Option<u64> {
    if token_amount == total_tokens {
        None
    } else {
        Some(token_amount)
    }
}

/// Build PTB for merging FSS and redeeming according to a `RedeemPlan`.
///
/// Common shape:
/// 1. Merge all `fss_refs` into a single FSS object.
/// 2. (Partial) `split_fungible_staked_sui` to a sub-FSS of `token_amount`.
/// 3. `redeem_fungible_staked_sui` → `Balance<SUI>`.
/// 4. (`AtLeast` only) `balance::split(&mut Balance<SUI>, min_sui)` → sub-balance,
///    then `balance::join` it back. The split aborts on chain when the redeemed
///    `Balance.value` is below `min_sui`, providing a runtime guard against
///    pool drift between simulate and execution. The join restores the original
///    balance so subsequent commands see a single, full `Balance<SUI>` again.
/// 5. `coin::from_balance<SUI>` → `Coin<SUI>`.
/// 6. `TransferObjects` → sender.
///
/// The `All` mode skips step 2 (redeems the merged FSS in full).
/// The `AtMost` mode performs step 2 with a token count chosen so that the
/// chain's `expected = floor(token * rate_sui / rate_token)` stays at or below
/// the user-supplied `max_sui`; the on-chain invariant `actual <= expected`
/// then guarantees the cap holds without an explicit guard.
pub fn merge_and_redeem_fss_pt(
    sender: SuiAddress,
    fss_refs: Vec<ObjectRef>,
    plan: &RedeemPlan,
) -> anyhow::Result<ProgrammableTransaction> {
    let mut builder = ProgrammableTransactionBuilder::new();

    if fss_refs.is_empty() {
        return Ok(builder.finish());
    }

    let system_state = builder.input(CallArg::SUI_SYSTEM_MUT)?;

    let merged_fss = builder.obj(ObjectArg::ImmOrOwnedObject(fss_refs[0]))?;
    for fss_ref in &fss_refs[1..] {
        let other = builder.obj(ObjectArg::ImmOrOwnedObject(*fss_ref))?;
        builder.command(Command::move_call(
            SUI_SYSTEM_PACKAGE_ID,
            Identifier::new("staking_pool")?,
            Identifier::new("join_fungible_staked_sui")?,
            vec![],
            vec![merged_fss, other],
        ));
    }

    // None = redeem the merged FSS in full (no split). Splitting when
    // token_amount equals the total would just leave a zero-value FSS as dust.
    let split_token_amount = match plan {
        RedeemPlan::All => None,
        RedeemPlan::AtLeast { token_amount, .. } | RedeemPlan::AtMost { token_amount, .. } => {
            *token_amount
        }
    };
    let redeem_target = if let Some(token_amount) = split_token_amount {
        let split_amount = builder.pure(token_amount)?;
        builder.command(Command::move_call(
            SUI_SYSTEM_PACKAGE_ID,
            Identifier::new("staking_pool")?,
            Identifier::new("split_fungible_staked_sui")?,
            vec![],
            vec![merged_fss, split_amount],
        ))
    } else {
        merged_fss
    };

    let balance_result = builder.command(Command::move_call(
        SUI_SYSTEM_PACKAGE_ID,
        SUI_SYSTEM_MODULE_NAME.to_owned(),
        Identifier::new("redeem_fungible_staked_sui")?,
        vec![],
        vec![system_state, redeem_target],
    ));

    let sui_type = sui_types::TypeTag::from_str("0x2::sui::SUI")?;

    if let RedeemPlan::AtLeast { min_sui, .. } = plan {
        let min_arg = builder.pure(*min_sui)?;
        let split_balance = builder.command(Command::move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            Identifier::new("balance")?,
            Identifier::new("split")?,
            vec![sui_type.clone()],
            vec![balance_result, min_arg],
        ));
        builder.command(Command::move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            Identifier::new("balance")?,
            Identifier::new("join")?,
            vec![sui_type.clone()],
            vec![balance_result, split_balance],
        ));
    }

    let coin_result = builder.command(Command::move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("coin")?,
        Identifier::new("from_balance")?,
        vec![sui_type],
        vec![balance_result],
    ));

    let sender_arg = builder.pure(sender)?;
    builder.command(Command::TransferObjects(vec![coin_result], sender_arg));

    Ok(builder.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- AtMost binary search -------------------------------------------------

    #[test]
    fn atmost_picks_maximum_token_under_cap() {
        // rate 200/99, supply=99: for T=49 expected=98, for T=50 expected=101.
        // Largest T with expected(T) <= 100 is 49.
        let token = binary_search_at_most(200, 99, 99, 100).unwrap().unwrap();
        assert_eq!(token, 49);
        assert!(expected_sui_amount(200, 99, token).unwrap() <= 100);
        assert!(expected_sui_amount(200, 99, token + 1).unwrap() > 100);
    }

    #[test]
    fn atmost_returns_none_when_one_token_already_over_cap() {
        // rate 1000:1 means 1 token = 1000 SUI; cap of 1 SUI is unsatisfiable.
        assert!(binary_search_at_most(1000, 1, 100, 1).unwrap().is_none());
    }

    #[test]
    fn atmost_zero_rate_falls_back_to_one_to_one() {
        // expected(token) = token when rate fields are 0. With cap=10 and 100
        // tokens available, max satisfying token is 10.
        assert_eq!(binary_search_at_most(0, 0, 100, 10).unwrap().unwrap(), 10);
    }

    #[test]
    fn mul_div_overflow_is_reported_not_truncated() {
        // u64::MAX * u64::MAX / 1 fits in u128 but NOT in u64 — must error,
        // not silently truncate. Confirms we match Move's `mul_div!` abort.
        let err = mul_div_u64(u64::MAX, u64::MAX, 1).expect_err("should overflow");
        assert!(format!("{err}").contains("overflow"));
    }

    #[test]
    fn mul_div_safely_handles_typical_pool_scale() {
        // Typical pool: ~10^9 SUI total, 1:1 rate. Should not overflow.
        let result = mul_div_u64(1_000_000_000_000_000_000, 1, 1).unwrap();
        assert_eq!(result, 1_000_000_000_000_000_000);
    }

    // --- Exchange-rate walk-back (mirrors pool_token_exchange_rate_at_epoch) -

    #[test]
    fn walk_back_active_pool_starts_at_current_epoch() {
        // Active pool (no deactivation): walk from current down to activation.
        let epochs = walk_back_epochs(Some(10), None, 15);
        assert_eq!(epochs, vec![15, 14, 13, 12, 11, 10]);
    }

    #[test]
    fn walk_back_inactive_pool_clamps_to_deactivation_epoch() {
        // Pool deactivated at epoch 12 — at current epoch 17 the chain replays
        // the rate snapshot at deactivation, walking back from there.
        let epochs = walk_back_epochs(Some(10), Some(12), 17);
        assert_eq!(epochs, vec![12, 11, 10]);
    }

    #[test]
    fn walk_back_clamp_is_minimum_of_deactivation_and_current() {
        // Pool deactivated *after* current epoch (unusual but the chain handles
        // it via min(deactivation, current)) — walk-back starts from current.
        let epochs = walk_back_epochs(Some(10), Some(99), 12);
        assert_eq!(epochs, vec![12, 11, 10]);
    }

    #[test]
    fn walk_back_preactive_returns_empty() {
        // Pool not yet activated at the lookup epoch → chain short-circuits to
        // initial_exchange_rate(). Empty list signals the same — no
        // exchange_rates entry to probe; the `(0, 0)` fallback applies.
        let epochs = walk_back_epochs(Some(20), None, 15);
        assert!(epochs.is_empty(), "got {epochs:?}");
    }

    #[test]
    fn walk_back_no_activation_returns_empty() {
        // Activation epoch is None — chain treats this as preactive.
        let epochs = walk_back_epochs(None, None, 5);
        assert!(epochs.is_empty(), "got {epochs:?}");
    }

    #[test]
    fn walk_back_single_epoch_pool() {
        // Pool activated and deactivated in the same epoch: walk-back probes
        // exactly that epoch.
        let epochs = walk_back_epochs(Some(7), Some(7), 30);
        assert_eq!(epochs, vec![7]);
    }

    // --- Plan construction ---------------------------------------------------

    fn fss(rate_sui: u64, rate_token: u64, principal: u64, supply: u64) -> PoolRedeemData {
        PoolRedeemData {
            rate_sui,
            rate_token,
            fss_principal: principal,
            fss_total_supply: supply,
        }
    }

    #[test]
    fn build_plan_atleast_picks_minimum_token_satisfying_actual() {
        // Pool: 1:1 rate, 100 supply, 50 principal.
        //   total_sui = floor(100*100/100) = 100
        //   principal_capped = 50, rewards = 50
        //   actual(t) = floor(t*50/100) + floor(t*50/100)
        // For t=5: actual = 2 + 2 = 4 (split-floor underrun!)
        // For t=6: actual = 3 + 3 = 6 ≥ 5 ✓
        // The naive `expected = floor(5*100/100) = 5` ceil formula would have
        // picked t=5, but actual=4 < 5 — the chain guard would abort. The
        // mirror search correctly picks t=6.
        let data = fss(100, 100, 50, 100);
        let plan =
            build_redeem_plan(RedeemMode::AtLeast, Some(5), 100, 100, 100, Some(&data)).unwrap();
        match plan {
            RedeemPlan::AtLeast {
                token_amount,
                min_sui,
            } => {
                let inner = token_amount.expect("t=6 < total=100, plan keeps Some(token)");
                assert_eq!(
                    inner, 6,
                    "mirror picks t=6 (actual=6) over naive t=5 (actual=4)"
                );
                assert_eq!(min_sui, 5);
                assert!(mirror_redeem_actual(&data, inner).unwrap() >= 5);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn build_plan_atleast_avoids_split_floor_underrun() {
        // Reproduce the production bug class: rates / FSS state where the simple
        // expected-rate ceiling underestimates by 1 MIST. Pick a pool where
        // mirror(t) = expected(t) - 1 for the candidate t, so a naive ceil
        // formula would pick t but actual(t) < min_sui.
        //
        // Setup: rate 7:3 (sui:token), supply 30, principal 7. Each token
        // expects floor(1*7/3) = 2 MIST cap, but split-floor accounting gives
        //   total_sui = floor(7*30/3) = 70
        //   principal_capped = min(7, 70) = 7, rewards = 63
        //   for t=15: principal_out=floor(15*7/30)=3, rewards_out=floor(15*63/30)=31, actual=34
        //   for t=16: principal_out=floor(16*7/30)=3, rewards_out=floor(16*63/30)=33, actual=36
        // Asking for min_sui=35: naive ceil(35*3/7) = 15 picks t=15 → actual=34 < 35 → guard would abort.
        // Mirror search picks t=16 → actual=36 >= 35 ✓.
        let data = fss(7, 3, 7, 30);
        let plan = build_redeem_plan(RedeemMode::AtLeast, Some(35), 30, 7, 3, Some(&data)).unwrap();
        match plan {
            RedeemPlan::AtLeast { token_amount, .. } => {
                let inner = token_amount.expect("t=16 < total=30, plan keeps Some(token)");
                assert_eq!(inner, 16, "mirror search should pick t=16, not naive t=15");
                assert!(mirror_redeem_actual(&data, inner).unwrap() >= 35);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn build_plan_atleast_rejects_when_insufficient_pool() {
        // Need 1000 SUI from a pool with 10 tokens at 1:1 — impossible.
        let data = fss(1, 1, 5, 10);
        let err = build_redeem_plan(RedeemMode::AtLeast, Some(1000), 10, 1, 1, Some(&data))
            .expect_err("should fail");
        assert!(format!("{err}").contains("Insufficient FSS balance"));
    }

    #[test]
    fn build_plan_atleast_rejects_zero_amount() {
        let data = fss(200, 100, 50, 100);
        let err = build_redeem_plan(RedeemMode::AtLeast, Some(0), 100, 200, 100, Some(&data))
            .expect_err("should fail");
        assert!(format!("{err}").contains("at least 1 MIST"));
    }

    #[test]
    fn build_plan_atleast_rejects_missing_fss_data() {
        let err = build_redeem_plan(RedeemMode::AtLeast, Some(5), 100, 200, 100, None)
            .expect_err("should fail");
        assert!(format!("{err}").contains("FungibleStakedSuiData required"));
    }

    #[test]
    fn build_plan_atmost_uses_binary_search() {
        let plan = build_redeem_plan(RedeemMode::AtMost, Some(100), 99, 200, 99, None).unwrap();
        match plan {
            RedeemPlan::AtMost {
                token_amount,
                max_sui,
            } => {
                assert_eq!(token_amount, Some(49));
                assert_eq!(max_sui, 100);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn build_plan_all_returns_all() {
        let plan = build_redeem_plan(RedeemMode::All, None, 100, 200, 100, None).unwrap();
        assert!(matches!(plan, RedeemPlan::All));
    }

    #[test]
    fn build_plan_atleast_full_redeem_omits_split() {
        // Pool of 100 supply at 1:1 — to deliver 100 SUI we need all 100 tokens.
        // binary_search returns total_tokens, so the plan should set
        // `token_amount = None` and the PTB will skip `split_fungible_staked_sui`.
        let data = fss(100, 100, 100, 100);
        let plan =
            build_redeem_plan(RedeemMode::AtLeast, Some(100), 100, 100, 100, Some(&data)).unwrap();
        match plan {
            RedeemPlan::AtLeast {
                token_amount,
                min_sui,
            } => {
                assert!(
                    token_amount.is_none(),
                    "full-redeem AtLeast should encode no-split as None, got {token_amount:?}"
                );
                assert_eq!(min_sui, 100);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn build_plan_atmost_full_redeem_omits_split() {
        // Cap of 1_000_000 at rate 1:1 with 99 total tokens — every token
        // fits, so `binary_search_at_most` returns total_tokens. Plan should
        // encode this as no-split.
        let plan = build_redeem_plan(RedeemMode::AtMost, Some(1_000_000), 99, 1, 1, None).unwrap();
        match plan {
            RedeemPlan::AtMost {
                token_amount,
                max_sui,
            } => {
                assert!(
                    token_amount.is_none(),
                    "full-redeem AtMost should encode no-split as None, got {token_amount:?}"
                );
                assert_eq!(max_sui, 1_000_000);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn mirror_invariant_actual_le_expected_with_small_gap() {
        // Property check: mirror_redeem_actual <= expected_sui_amount across a
        // grid of plausible pool states, with the gap bounded by ~2 MIST.
        for rate_sui in [100u64, 199, 1024, 100_003] {
            for rate_token in [99u64, 100, 1023, 99_997] {
                let supply = rate_token;
                for principal_pct in [0u64, 30, 50, 80, 100] {
                    let principal = supply * principal_pct / 100;
                    let data = fss(rate_sui, rate_token, principal, supply);
                    for token in [1u64, 2, supply / 2, supply.saturating_sub(1), supply] {
                        if token == 0 || token > supply {
                            continue;
                        }
                        let expected = expected_sui_amount(rate_sui, rate_token, token).unwrap();
                        let actual = mirror_redeem_actual(&data, token).unwrap();
                        assert!(actual <= expected, "actual={actual} > expected={expected}");
                        assert!(
                            actual + 2 >= expected,
                            "gap > 2 MIST: expected={expected} actual={actual}"
                        );
                    }
                }
            }
        }
    }

    // --- PTB shape -----------------------------------------------------------

    fn ref_(id: u8) -> ObjectRef {
        use sui_types::base_types::{ObjectDigest, SequenceNumber};
        let mut bytes = [0u8; 32];
        bytes[31] = id;
        (
            ObjectID::from_bytes(bytes).unwrap(),
            SequenceNumber::from_u64(1),
            ObjectDigest::random(),
        )
    }

    #[test]
    fn ptb_all_omits_split_and_guard() {
        let sender = SuiAddress::random_for_testing_only();
        let pt = merge_and_redeem_fss_pt(sender, vec![ref_(1)], &RedeemPlan::All).unwrap();

        let move_calls: Vec<&str> = pt
            .commands
            .iter()
            .filter_map(|c| match c {
                Command::MoveCall(m) => Some(m.function.as_str()),
                _ => None,
            })
            .collect();
        assert!(move_calls.contains(&"redeem_fungible_staked_sui"));
        assert!(move_calls.contains(&"from_balance"));
        assert!(!move_calls.contains(&"split_fungible_staked_sui"));
        assert!(!move_calls.contains(&"split"));
        assert!(!move_calls.contains(&"join"));
    }

    #[test]
    fn ptb_atleast_includes_balance_split_join_guard() {
        let sender = SuiAddress::random_for_testing_only();
        let plan = RedeemPlan::AtLeast {
            token_amount: Some(100),
            min_sui: 50,
        };
        let pt = merge_and_redeem_fss_pt(sender, vec![ref_(1), ref_(2)], &plan).unwrap();

        let move_calls: Vec<(&str, &str)> = pt
            .commands
            .iter()
            .filter_map(|c| match c {
                Command::MoveCall(m) => Some((m.module.as_str(), m.function.as_str())),
                _ => None,
            })
            .collect();
        assert!(move_calls.contains(&("staking_pool", "join_fungible_staked_sui")));
        assert!(move_calls.contains(&("staking_pool", "split_fungible_staked_sui")));
        assert!(move_calls.contains(&("sui_system", "redeem_fungible_staked_sui")));
        assert!(move_calls.contains(&("balance", "split")));
        assert!(move_calls.contains(&("balance", "join")));
        assert!(move_calls.contains(&("coin", "from_balance")));
    }

    #[test]
    fn ptb_atmost_omits_balance_guard() {
        let sender = SuiAddress::random_for_testing_only();
        let plan = RedeemPlan::AtMost {
            token_amount: Some(100),
            max_sui: 50,
        };
        let pt = merge_and_redeem_fss_pt(sender, vec![ref_(1)], &plan).unwrap();

        let move_calls: Vec<(&str, &str)> = pt
            .commands
            .iter()
            .filter_map(|c| match c {
                Command::MoveCall(m) => Some((m.module.as_str(), m.function.as_str())),
                _ => None,
            })
            .collect();
        assert!(move_calls.contains(&("staking_pool", "split_fungible_staked_sui")));
        assert!(move_calls.contains(&("sui_system", "redeem_fungible_staked_sui")));
        assert!(!move_calls.contains(&("balance", "split")));
        assert!(!move_calls.contains(&("balance", "join")));
        assert!(move_calls.contains(&("coin", "from_balance")));
    }

    #[test]
    fn ptb_atleast_full_redeem_omits_split_keeps_guard() {
        // token_amount = None → no `split_fungible_staked_sui`. The chain
        // guard (balance::split + balance::join) stays in place because
        // AtLeast still needs runtime under-delivery protection regardless
        // of whether we're redeeming all or a subset.
        let sender = SuiAddress::random_for_testing_only();
        let plan = RedeemPlan::AtLeast {
            token_amount: None,
            min_sui: 50,
        };
        let pt = merge_and_redeem_fss_pt(sender, vec![ref_(1)], &plan).unwrap();

        let move_calls: Vec<(&str, &str)> = pt
            .commands
            .iter()
            .filter_map(|c| match c {
                Command::MoveCall(m) => Some((m.module.as_str(), m.function.as_str())),
                _ => None,
            })
            .collect();
        assert!(
            !move_calls.contains(&("staking_pool", "split_fungible_staked_sui")),
            "full-redeem AtLeast should not split FSS: {move_calls:?}"
        );
        assert!(move_calls.contains(&("sui_system", "redeem_fungible_staked_sui")));
        assert!(move_calls.contains(&("balance", "split")));
        assert!(move_calls.contains(&("balance", "join")));
        assert!(move_calls.contains(&("coin", "from_balance")));
    }

    #[test]
    fn ptb_atmost_full_redeem_omits_split_no_guard() {
        // token_amount = None → no split, no balance guard. Bytes-on-chain
        // are identical to a `RedeemPlan::All` PTB; the difference is only
        // in `bind_epoch` which the parser can't recover, so this round-trips
        // through parse as `Some(All)`.
        let sender = SuiAddress::random_for_testing_only();
        let plan = RedeemPlan::AtMost {
            token_amount: None,
            max_sui: 1_000_000,
        };
        let pt = merge_and_redeem_fss_pt(sender, vec![ref_(1)], &plan).unwrap();

        let move_calls: Vec<(&str, &str)> = pt
            .commands
            .iter()
            .filter_map(|c| match c {
                Command::MoveCall(m) => Some((m.module.as_str(), m.function.as_str())),
                _ => None,
            })
            .collect();
        assert!(!move_calls.contains(&("staking_pool", "split_fungible_staked_sui")));
        assert!(!move_calls.contains(&("balance", "split")));
        assert!(!move_calls.contains(&("balance", "join")));
        assert!(move_calls.contains(&("sui_system", "redeem_fungible_staked_sui")));
        assert!(move_calls.contains(&("coin", "from_balance")));
    }
}
