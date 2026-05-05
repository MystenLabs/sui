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
use sui_rpc::proto::sui::rpc::v2::ListOwnedObjectsRequest;
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
// Mirror of the on-chain exchange-rate formula (pure, no RPC).
// ============================================================================

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

/// Snapshot of an active validator pulled out of `system_state.validators.active_validators`.
#[derive(Clone, Debug)]
struct ActivePoolEntry {
    pool_id: ObjectID,
    validator_addr: SuiAddress,
}

/// Read active validators and the inactive_validators table id from the system state.
async fn fetch_validator_index(
    client: &mut Client,
) -> Result<(Vec<ActivePoolEntry>, Option<ObjectID>), Error> {
    use sui_rpc::proto::sui::rpc::v2::GetEpochRequest;
    let request = GetEpochRequest::latest().with_read_mask(FieldMask::from_paths([
        "system_state.validators.active_validators",
        "system_state.validators.inactive_validators",
    ]));
    let response = client
        .ledger_client()
        .get_epoch(request)
        .await?
        .into_inner();
    let validators = response.epoch().system_state().validators();

    let mut active = Vec::new();
    for v in validators.active_validators() {
        let pool_id = ObjectID::from_str(v.staking_pool().id())
            .map_err(|e| Error::DataError(format!("Invalid active pool id: {e}")))?;
        let validator_addr = SuiAddress::from_str(v.address())
            .map_err(|e| Error::DataError(format!("Invalid active validator address: {e}")))?;
        active.push(ActivePoolEntry {
            pool_id,
            validator_addr,
        });
    }

    let inactive_table_id = validators
        .inactive_validators
        .as_ref()
        .and_then(|t| t.id.as_ref())
        .and_then(|id| ObjectID::from_str(id).ok());

    Ok((active, inactive_table_id))
}

/// Resolve `validator` to a pool plus the sender's FSS in that pool.
///
/// Uses the active validator set to map `validator → pool_id`, then groups the
/// sender's FSS by pool. Returns `(pool_id, fss_refs, total_tokens)`.
///
/// Inactive validators (deactivated pools) are not currently supported by this
/// path; the chain still allows redeeming from inactive pools, but the lookup
/// requires walking `inactive_validators[pool_id] → ValidatorWrapper` dynamic
/// fields, which is a follow-up.
async fn resolve_pool_and_fss(
    client: &mut Client,
    sender: SuiAddress,
    target_validator: SuiAddress,
) -> Result<(ObjectID, Vec<ObjectRef>, u64), Error> {
    let owned = list_owned_fss(client, sender).await?;
    if owned.is_empty() {
        return Err(Error::InvalidInput(format!(
            "No FungibleStakedSui found for sender {sender}"
        )));
    }

    let (active, _inactive_table_id) = fetch_validator_index(client).await?;
    let pool_for_validator = active
        .iter()
        .find(|v| v.validator_addr == target_validator)
        .map(|v| v.pool_id)
        .ok_or_else(|| {
            Error::InvalidInput(format!(
                "Validator {target_validator} not found among active validators"
            ))
        })?;

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
    let refs: Vec<ObjectRef> = matching.iter().map(|f| f.object_ref).collect();
    Ok((pool_for_validator, refs, total_tokens))
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

        let (pool_id, fss_refs, total_tokens) =
            resolve_pool_and_fss(client, sender, validator).await?;

        let (rate_sui, rate_token) = pool_rate(client, pool_id).await?;

        let plan = build_redeem_plan(redeem_mode, amount, total_tokens, rate_sui, rate_token)?;
        let bind_epoch = match plan {
            RedeemPlan::All => None,
            RedeemPlan::AtLeast { .. } | RedeemPlan::AtMost { .. } => {
                Some(crate::get_current_epoch(client).await?)
            }
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

/// Translate a user-supplied `(redeem_mode, amount)` pair into a `RedeemPlan`,
/// validating amounts and computing token counts via binary search over the
/// chain's exchange-rate formula.
pub(crate) fn build_redeem_plan(
    redeem_mode: RedeemMode,
    amount: Option<u64>,
    total_tokens: u64,
    rate_sui: u64,
    rate_token: u64,
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
            let numerator = (min_sui as u128) * (rate_token as u128) + (rate_sui as u128) - 1;
            let tokens = (numerator / rate_sui as u128) as u64;
            if tokens > total_tokens {
                return Err(Error::InvalidInput(format!(
                    "Insufficient FSS balance: AtLeast {min_sui} SUI requires {tokens} tokens \
                     but only {total_tokens} available",
                )));
            }
            Ok(RedeemPlan::AtLeast {
                token_amount: tokens,
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

/// Resolve a pool's `(rate_sui, rate_token)` for the latest exchange rate.
///
/// The staking pool's `sui_balance` and `pool_token_balance` fields equal the
/// latest exchange rate's `(sui_amount, pool_token_amount)` within an epoch,
/// since exchange rates are only added at epoch boundaries (see
/// `staking_pool.move::process_pending_stakes_and_withdraws`).
async fn pool_rate(client: &mut Client, pool_id: ObjectID) -> Result<(u64, u64), Error> {
    let pool_rates = crate::account::get_pool_exchange_rates(client).await?;
    let rate = pool_rates.get(&pool_id.to_string()).ok_or_else(|| {
        Error::DataError(format!(
            "No exchange rate found for pool {pool_id} \
             (validator may be inactive — only active validators are supported \
             for redeeming FungibleStakedSui)"
        ))
    })?;
    Ok((rate.sui_balance, rate.pool_token_balance))
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

    #[test]
    fn build_plan_atleast_picks_ceiling() {
        // rate 200:100 means 1 token = 2 SUI. Need 5 SUI → ceil(5*100/200)=3.
        let plan = build_redeem_plan(RedeemMode::AtLeast, Some(5), 100, 200, 100).unwrap();
        match plan {
            RedeemPlan::AtLeast {
                token_amount,
                min_sui,
            } => {
                assert_eq!(token_amount, 3);
                assert_eq!(min_sui, 5);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn build_plan_atleast_rejects_when_insufficient_pool() {
        // Need 1000 SUI from a pool with 10 tokens at 1:1 — impossible.
        let err =
            build_redeem_plan(RedeemMode::AtLeast, Some(1000), 10, 1, 1).expect_err("should fail");
        assert!(format!("{err}").contains("Insufficient FSS balance"));
    }

    #[test]
    fn build_plan_atleast_rejects_zero_amount() {
        let err = build_redeem_plan(RedeemMode::AtLeast, Some(0), 100, 200, 100)
            .expect_err("should fail");
        assert!(format!("{err}").contains("at least 1 MIST"));
    }

    #[test]
    fn build_plan_atmost_uses_binary_search() {
        let plan = build_redeem_plan(RedeemMode::AtMost, Some(100), 99, 200, 99).unwrap();
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
        let plan = build_redeem_plan(RedeemMode::All, None, 100, 200, 100).unwrap();
        assert!(matches!(plan, RedeemPlan::All));
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
