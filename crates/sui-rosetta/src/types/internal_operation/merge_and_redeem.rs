// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

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
use crate::types::RedeemMode;

use super::{
    TransactionObjectData, TryConstructTransaction, get_validator_pool_id, simulate_transaction,
};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MergeAndRedeemFungibleStakedSui {
    pub sender: SuiAddress,
    pub validator: SuiAddress,
    pub amount: Option<u64>,
    pub redeem_mode: RedeemMode,
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

        let pool_id = get_validator_pool_id(client, validator).await?;

        // Discover FSS objects for this validator's pool
        let list_request = ListOwnedObjectsRequest::default()
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
            .list_owned_objects(list_request)
            .map_err(Error::from)
            .try_collect()
            .await?;

        let mut fss_refs = Vec::new();
        let mut total_tokens: u64 = 0;
        for obj in &objects {
            let contents = obj.contents.as_ref().ok_or_else(|| {
                Error::DataError("FungibleStakedSui missing contents".to_string())
            })?;
            let fss: FungibleStakedSuiBcs = contents.deserialize().map_err(|e| {
                Error::DataError(format!("Failed to deserialize FungibleStakedSui: {}", e))
            })?;

            if fss.pool_id.to_string() == pool_id {
                total_tokens += fss.value;
                fss_refs.push((
                    ObjectID::from_str(obj.object_id())
                        .map_err(|e| Error::DataError(format!("Invalid object_id: {}", e)))?,
                    obj.version().into(),
                    obj.digest()
                        .parse()
                        .map_err(|e| Error::DataError(format!("Invalid digest: {}", e)))?,
                ));
            }
        }

        if fss_refs.is_empty() {
            return Err(Error::InvalidInput(format!(
                "No FungibleStakedSui found for validator {}",
                validator,
            )));
        }

        // Read exchange rate to convert SUI amount → pool tokens.
        // Note: the rate is read at metadata time. If the TX lands in a later epoch,
        // the on-chain rate may differ. AtLeast mode may yield slightly more SUI than
        // requested; AtMost may yield slightly less. The actual redeemed amount is
        // captured in the TX's balance_changes.
        let pool_rates = crate::account::get_pool_exchange_rates(client).await?;
        let rate = pool_rates.get(&pool_id).ok_or_else(|| {
            Error::DataError(format!("No exchange rate found for pool {}", pool_id))
        })?;
        let (sui_balance, pool_token_balance) = (rate.sui_balance, rate.pool_token_balance);

        let token_amount = match redeem_mode {
            RedeemMode::All => total_tokens,
            RedeemMode::AtLeast => {
                let amount = amount.ok_or_else(|| {
                    Error::InvalidInput("amount required for AtLeast mode".to_string())
                })?;
                // ceil(amount * pool_token_balance / sui_balance)
                if sui_balance == 0 {
                    return Err(Error::DataError(
                        "Pool has zero SUI balance, cannot compute exchange rate".to_string(),
                    ));
                }
                let numerator =
                    amount as u128 * pool_token_balance as u128 + sui_balance as u128 - 1;
                let tokens = (numerator / sui_balance as u128) as u64;
                if tokens > total_tokens {
                    return Err(Error::InvalidInput(format!(
                        "Insufficient FSS balance: AtLeast {} SUI requires {} tokens but only {} available",
                        amount, tokens, total_tokens,
                    )));
                }
                tokens
            }
            RedeemMode::AtMost => {
                let amount = amount.ok_or_else(|| {
                    Error::InvalidInput("amount required for AtMost mode".to_string())
                })?;
                // floor(amount * pool_token_balance / sui_balance)
                if sui_balance == 0 {
                    return Err(Error::DataError(
                        "Pool has zero SUI balance, cannot compute exchange rate".to_string(),
                    ));
                }
                let tokens =
                    (amount as u128 * pool_token_balance as u128 / sui_balance as u128) as u64;
                if tokens == 0 {
                    return Err(Error::InvalidInput(
                        "AtMost amount too small: rounds to 0 pool tokens".to_string(),
                    ));
                }
                tokens.min(total_tokens)
            }
        };

        let is_redeem_all = token_amount == total_tokens;
        let pt_token_amount = if is_redeem_all {
            None
        } else {
            Some(token_amount)
        };
        let pt = merge_and_redeem_fss_pt(sender, fss_refs.clone(), pt_token_amount)?;
        let (budget, gas_coin_objs) =
            simulate_transaction(client, pt, sender, vec![], gas_price, budget).await?;

        let total_sui_balance = gas_coin_objs.iter().map(|c| c.balance()).sum::<u64>() as i128;
        let gas_coins = gas_coin_objs
            .iter()
            .map(|obj| obj.object_reference().try_to_object_ref())
            .collect::<Result<Vec<_>, _>>()?;

        Ok(TransactionObjectData {
            gas_coins,
            objects: fss_refs,
            party_objects: vec![],
            total_sui_balance,
            budget,
            address_balance_withdrawal: 0,
            fss_object_count: None,
            redeem_token_amount: if is_redeem_all {
                None
            } else {
                Some(token_amount)
            },
        })
    }
}

/// Build PTB for merging all FSS and redeeming.
///
/// Phase 1: Merge all FSS into one
/// Phase 2: Split token amount (None = redeem all, Some(n) = split n tokens)
/// Phase 3: redeem_fungible_staked_sui → Balance<SUI>
/// Phase 4: coin::from_balance<SUI> → Coin<SUI>
/// Phase 5: TransferObjects → sender
pub fn merge_and_redeem_fss_pt(
    sender: SuiAddress,
    fss_refs: Vec<ObjectRef>,
    token_amount: Option<u64>,
) -> anyhow::Result<ProgrammableTransaction> {
    let mut builder = ProgrammableTransactionBuilder::new();

    if fss_refs.is_empty() {
        return Ok(builder.finish());
    }

    let system_state = builder.input(CallArg::SUI_SYSTEM_MUT)?;

    // Phase 1: Merge all FSS into the first one using staking_pool::join_fungible_staked_sui
    // MergeCoins only works on Coin<T>, not FungibleStakedSui
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

    // Phase 2: Split or use whole FSS
    // FSS is not a Coin, so we must use staking_pool::split_fungible_staked_sui instead of SplitCoins
    let redeem_target = if let Some(amount) = token_amount {
        let split_amount = builder.pure(amount)?;
        builder.command(Command::move_call(
            SUI_SYSTEM_PACKAGE_ID,
            Identifier::new("staking_pool")?,
            Identifier::new("split_fungible_staked_sui")?,
            vec![],
            vec![merged_fss, split_amount],
        ))
    } else {
        // Redeem all — use the entire merged FSS
        merged_fss
    };

    // Phase 3: Redeem FSS → Balance<SUI>
    let balance_result = builder.command(Command::move_call(
        SUI_SYSTEM_PACKAGE_ID,
        SUI_SYSTEM_MODULE_NAME.to_owned(),
        Identifier::new("redeem_fungible_staked_sui")?,
        vec![],
        vec![system_state, redeem_target],
    ));

    // Phase 4: Convert Balance<SUI> → Coin<SUI>
    let sui_type = sui_types::TypeTag::from_str("0x2::sui::SUI")?;
    let coin_result = builder.command(Command::move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("coin")?,
        Identifier::new("from_balance")?,
        vec![sui_type],
        vec![balance_result],
    ));

    // Phase 5: Transfer Coin<SUI> to sender
    let sender_arg = builder.pure(sender)?;
    builder.command(Command::TransferObjects(vec![coin_result], sender_arg));

    Ok(builder.finish())
}
