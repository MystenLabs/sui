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
pub(crate) fn expected_sui_amount(rate_sui: u64, rate_token: u64, token_amount: u64) -> u64 {
    if rate_sui == 0 || rate_token == 0 {
        return token_amount;
    }
    ((token_amount as u128) * (rate_sui as u128) / (rate_token as u128)) as u64
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
pub(crate) fn mirror_redeem_actual(data: &PoolRedeemData, token_amount: u64) -> u64 {
    if data.fss_total_supply == 0 {
        return 0;
    }

    // total_sui = exchange_rate.get_sui_amount(total_supply)
    let total_sui = if data.rate_sui == 0 || data.rate_token == 0 {
        data.fss_total_supply
    } else {
        ((data.rate_sui as u128) * (data.fss_total_supply as u128) / (data.rate_token as u128))
            as u64
    };

    let principal = data.fss_principal.min(total_sui);
    let rewards = total_sui - principal;

    let principal_out =
        ((token_amount as u128) * (principal as u128) / (data.fss_total_supply as u128)) as u64;
    let rewards_out =
        ((token_amount as u128) * (rewards as u128) / (data.fss_total_supply as u128)) as u64;

    principal_out + rewards_out
}

/// Binary-search the smallest `token_amount` in `[1, total_tokens]` such that
/// `mirror_redeem_actual(data, token_amount) >= target_sui`.
///
/// Returns `None` if even redeeming `total_tokens` produces less than
/// `target_sui` (caller reports "Insufficient FSS balance").
pub(crate) fn binary_search_at_least(
    data: &PoolRedeemData,
    total_tokens: u64,
    target_sui: u64,
) -> Option<u64> {
    if total_tokens == 0 {
        return None;
    }
    if mirror_redeem_actual(data, total_tokens) < target_sui {
        return None;
    }
    let mut lo: u64 = 1;
    let mut hi: u64 = total_tokens;
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if mirror_redeem_actual(data, mid) >= target_sui {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    Some(lo)
}

/// Binary-search the largest `token_amount` in `[1, total_tokens]` such that
/// `expected_sui_amount(rate_sui, rate_token, token_amount) <= max_sui`.
///
/// Returns `None` if even one token would already exceed `max_sui` (caller
/// reports "AtMost amount too small to redeem any tokens").
pub(crate) fn binary_search_at_most(
    rate_sui: u64,
    rate_token: u64,
    total_tokens: u64,
    max_sui: u64,
) -> Option<u64> {
    if total_tokens == 0 {
        return None;
    }
    if expected_sui_amount(rate_sui, rate_token, 1) > max_sui {
        return None;
    }
    let mut lo: u64 = 1;
    let mut hi: u64 = total_tokens;
    let mut ans: Option<u64> = None;
    while lo <= hi {
        let mid = lo + (hi - lo) / 2;
        if expected_sui_amount(rate_sui, rate_token, mid) <= max_sui {
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
    ans
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
/// `(rate_sui, rate_token, epoch)` snapshot — all from a single
/// `GetEpochRequest::latest()` response.
///
/// **Atomicity is load-bearing**: amount-sensitive plans bind the resulting
/// transaction to `epoch`, so the rate must come from the same epoch. Splitting
/// this into separate RPCs creates a race where rate is from epoch N but
/// `bind_epoch` is N+1, silently violating AtMost caps and aborting AtLeast
/// guards.
///
/// Inactive validators (deactivated pools) are not supported by this path;
/// the chain still allows redeeming from inactive pools, but the lookup
/// requires walking `inactive_validators[pool_id] → ValidatorWrapper` dynamic
/// fields, which is a follow-up.
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

    let (rates, epoch) = crate::account::get_pool_exchange_rates_with_epoch(client).await?;

    let target_validator_addr = sui_sdk_types::Address::from(target_validator);
    let (pool_id_str, rate_info) = rates
        .iter()
        .find(|(_, info)| info.validator_address == target_validator_addr)
        .ok_or_else(|| {
            Error::InvalidInput(format!(
                "Validator {target_validator} not found among active validators"
            ))
        })?;
    let pool_for_validator = ObjectID::from_str(pool_id_str)
        .map_err(|e| Error::DataError(format!("Invalid active pool id: {e}")))?;

    let mut matching: Vec<&OwnedFss> = owned
        .iter()
        .filter(|f| f.pool_id == pool_for_validator)
        .collect();
    matching.sort_by_key(|f| f.object_ref.0);

    if matching.is_empty() {
        return Err(Error::InvalidInput(format!(
            "Sender {sender} has no FungibleStakedSui for validator {target_validator}'s pool"
        )));
    }

    let total_tokens: u64 = matching.iter().map(|f| f.value).sum();
    let fss_refs: Vec<ObjectRef> = matching.iter().map(|f| f.object_ref).collect();
    let pool_extra_fields_id = rate_info
        .pool_extra_fields_id
        .as_deref()
        .and_then(|s| ObjectID::from_str(s).ok());
    Ok(PoolFssRateSnapshot {
        fss_refs,
        total_tokens,
        rate_sui: rate_info.sui_balance,
        rate_token: rate_info.pool_token_balance,
        epoch,
        pool_extra_fields_id,
    })
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
                Some(token_amount)
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
                binary_search_at_least(data, total_tokens, min_sui).ok_or_else(|| {
                    Error::InvalidInput(format!(
                        "Insufficient FSS balance: cannot deliver AtLeast {min_sui} SUI \
                         from {total_tokens} pool tokens at current exchange rate",
                    ))
                })?;
            Ok(RedeemPlan::AtLeast {
                token_amount,
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
            let token_amount = binary_search_at_most(rate_sui, rate_token, total_tokens, max_sui)
                .ok_or_else(|| {
                Error::InvalidInput(
                    "AtMost amount too small: would redeem 0 pool tokens at current exchange rate"
                        .to_string(),
                )
            })?;
            Ok(RedeemPlan::AtMost {
                token_amount,
                max_sui,
            })
        }
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

    let split_token_amount = match plan {
        RedeemPlan::All => None,
        RedeemPlan::AtLeast { token_amount, .. } | RedeemPlan::AtMost { token_amount, .. } => {
            Some(*token_amount)
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
        let token = binary_search_at_most(200, 99, 99, 100).unwrap();
        assert_eq!(token, 49);
        assert!(expected_sui_amount(200, 99, token) <= 100);
        assert!(expected_sui_amount(200, 99, token + 1) > 100);
    }

    #[test]
    fn atmost_returns_none_when_one_token_already_over_cap() {
        // rate 1000:1 means 1 token = 1000 SUI; cap of 1 SUI is unsatisfiable.
        assert!(binary_search_at_most(1000, 1, 100, 1).is_none());
    }

    #[test]
    fn atmost_zero_rate_falls_back_to_one_to_one() {
        // expected(token) = token when rate fields are 0. With cap=10 and 100
        // tokens available, max satisfying token is 10.
        assert_eq!(binary_search_at_most(0, 0, 100, 10).unwrap(), 10);
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
                assert_eq!(
                    token_amount, 6,
                    "mirror picks t=6 (actual=6) over naive t=5 (actual=4)"
                );
                assert_eq!(min_sui, 5);
                assert!(mirror_redeem_actual(&data, token_amount) >= 5);
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
                assert_eq!(
                    token_amount, 16,
                    "mirror search should pick t=16, not naive t=15"
                );
                assert!(mirror_redeem_actual(&data, token_amount) >= 35);
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
                assert_eq!(token_amount, 49);
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
                        let expected = expected_sui_amount(rate_sui, rate_token, token);
                        let actual = mirror_redeem_actual(&data, token);
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
            token_amount: 100,
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
            token_amount: 100,
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
}
