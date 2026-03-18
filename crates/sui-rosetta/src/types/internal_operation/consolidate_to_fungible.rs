// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use futures::TryStreamExt;
use prost_types::FieldMask;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use sui_rpc::client::Client;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::{GetEpochRequest, ListOwnedObjectsRequest};
use sui_sdk_types::Address;
use sui_types::base_types::{ObjectID, ObjectRef, SuiAddress};
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::rpc_proto_conversions::ObjectReferenceExt;
use sui_types::sui_system_state::SUI_SYSTEM_MODULE_NAME;
use sui_types::transaction::{CallArg, Command, ObjectArg, ProgrammableTransaction};
use sui_types::{Identifier, SUI_SYSTEM_PACKAGE_ID};

use crate::errors::Error;

use super::{TransactionObjectData, TryConstructTransaction, simulate_transaction};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ConsolidateAllStakedSuiToFungible {
    pub sender: SuiAddress,
    pub validator: SuiAddress,
}

/// BCS layout for `0x3::staking_pool::StakedSui`.
/// Field order must match the Move struct definition exactly (BCS is positional).
/// See: crates/sui-framework/packages/sui-system/sources/staking_pool.move
#[derive(Deserialize)]
struct StakedSuiBcs {
    _id: Address,
    pool_id: Address,
    stake_activation_epoch: u64,
    _principal: u64,
}

/// BCS layout for `0x3::staking_pool::FungibleStakedSui`.
/// Field order must match the Move struct definition exactly (BCS is positional).
/// See: crates/sui-framework/packages/sui-system/sources/staking_pool.move
#[derive(Deserialize)]
struct FungibleStakedSuiBcs {
    _id: Address,
    pool_id: Address,
    _value: u64,
}

#[async_trait]
impl TryConstructTransaction for ConsolidateAllStakedSuiToFungible {
    async fn try_fetch_needed_objects(
        self,
        client: &mut Client,
        gas_price: Option<u64>,
        budget: Option<u64>,
    ) -> Result<TransactionObjectData, Error> {
        let Self { sender, validator } = self;

        let current_epoch = crate::get_current_epoch(client).await?;
        let pool_id = get_validator_pool_id(client, validator).await?;

        let staked_sui_refs = discover_staked_sui(client, sender, &pool_id, current_epoch).await?;
        let fss_refs = discover_fss(client, sender, &pool_id).await?;

        if staked_sui_refs.is_empty() && fss_refs.len() <= 1 {
            return Err(Error::InvalidInput(format!(
                "Nothing to consolidate for validator {}: {} activated StakedSui, {} FungibleStakedSui",
                validator,
                staked_sui_refs.len(),
                fss_refs.len(),
            )));
        }

        let total_commands = staked_sui_refs.len() * 2 + fss_refs.len() + 2;
        if total_commands > super::MAX_COMMAND_ARGS {
            return Err(Error::InvalidInput(format!(
                "Too many objects to consolidate ({} StakedSui + {} FSS). Maximum ~{} objects supported.",
                staked_sui_refs.len(),
                fss_refs.len(),
                super::MAX_COMMAND_ARGS / 2,
            )));
        }

        let fss_count = fss_refs.len();

        // Objects layout: FSS refs first, then StakedSui refs
        let mut all_objects = Vec::with_capacity(fss_refs.len() + staked_sui_refs.len());
        all_objects.extend_from_slice(&fss_refs);
        all_objects.extend_from_slice(&staked_sui_refs);

        let pt = consolidate_to_fungible_pt(sender, fss_refs, staked_sui_refs.clone())?;
        let (budget, gas_coin_objs) =
            simulate_transaction(client, pt, sender, vec![], gas_price, budget).await?;

        let total_sui_balance = gas_coin_objs.iter().map(|c| c.balance()).sum::<u64>() as i128;
        let gas_coins = gas_coin_objs
            .iter()
            .map(|obj| obj.object_reference().try_to_object_ref())
            .collect::<Result<Vec<_>, _>>()?;

        Ok(TransactionObjectData {
            gas_coins,
            objects: all_objects,
            party_objects: vec![],
            total_sui_balance,
            budget,
            address_balance_withdrawal: 0,
            fss_object_count: Some(fss_count as u64),
            redeem_token_amount: None,
        })
    }
}

async fn discover_staked_sui(
    client: &mut Client,
    sender: SuiAddress,
    pool_id: &str,
    current_epoch: u64,
) -> Result<Vec<ObjectRef>, Error> {
    let list_request = ListOwnedObjectsRequest::default()
        .with_owner(sender.to_string())
        .with_object_type("0x3::staking_pool::StakedSui".to_string())
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

    let mut refs = Vec::new();
    for obj in objects {
        let contents = obj
            .contents
            .as_ref()
            .ok_or_else(|| Error::DataError("StakedSui missing contents".to_string()))?;
        let staked: StakedSuiBcs = contents
            .deserialize()
            .map_err(|e| Error::DataError(format!("Failed to deserialize StakedSui: {}", e)))?;

        if staked.pool_id.to_string() == pool_id && current_epoch >= staked.stake_activation_epoch {
            refs.push((
                ObjectID::from_str(obj.object_id())
                    .map_err(|e| Error::DataError(format!("Invalid object_id: {}", e)))?,
                obj.version().into(),
                obj.digest()
                    .parse()
                    .map_err(|e| Error::DataError(format!("Invalid digest: {}", e)))?,
            ));
        }
    }
    Ok(refs)
}

async fn discover_fss(
    client: &mut Client,
    sender: SuiAddress,
    pool_id: &str,
) -> Result<Vec<ObjectRef>, Error> {
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

    let mut refs = Vec::new();
    for obj in objects {
        let contents = obj
            .contents
            .as_ref()
            .ok_or_else(|| Error::DataError("FungibleStakedSui missing contents".to_string()))?;
        let fss: FungibleStakedSuiBcs = contents.deserialize().map_err(|e| {
            Error::DataError(format!("Failed to deserialize FungibleStakedSui: {}", e))
        })?;

        if fss.pool_id.to_string() == pool_id {
            refs.push((
                ObjectID::from_str(obj.object_id())
                    .map_err(|e| Error::DataError(format!("Invalid object_id: {}", e)))?,
                obj.version().into(),
                obj.digest()
                    .parse()
                    .map_err(|e| Error::DataError(format!("Invalid digest: {}", e)))?,
            ));
        }
    }
    Ok(refs)
}

pub(crate) async fn get_validator_pool_id(
    client: &mut Client,
    validator: SuiAddress,
) -> Result<String, Error> {
    let request = GetEpochRequest::latest().with_read_mask(FieldMask::from_paths([
        "system_state.validators.active_validators",
    ]));
    let response = client
        .ledger_client()
        .get_epoch(request)
        .await?
        .into_inner();
    let validators = response
        .epoch()
        .system_state()
        .validators()
        .active_validators();

    for v in validators {
        if let Ok(addr) = v.address().parse::<SuiAddress>()
            && addr == validator
        {
            return Ok(v.staking_pool().id().to_string());
        }
    }
    Err(Error::InvalidInput(format!(
        "Validator {} not found in active validators",
        validator
    )))
}

/// Build PTB for consolidating StakedSui → FungibleStakedSui.
///
/// Phase 1: Merge existing FSS (if >1)
/// Phase 2: Convert each StakedSui → FSS
/// Phase 3: Merge all new FSS together (if >1)
/// Phase 4: Merge new into existing (if existing) or TransferObjects to sender
pub fn consolidate_to_fungible_pt(
    sender: SuiAddress,
    fss_refs: Vec<ObjectRef>,
    staked_sui_refs: Vec<ObjectRef>,
) -> anyhow::Result<ProgrammableTransaction> {
    let mut builder = ProgrammableTransactionBuilder::new();

    if fss_refs.is_empty() && staked_sui_refs.is_empty() {
        return Ok(builder.finish());
    }

    let system_state = builder.input(CallArg::SUI_SYSTEM_MUT)?;

    // Phase 1: Merge existing FSS into the first one using staking_pool::join_fungible_staked_sui
    // MergeCoins only works on Coin<T>, not FungibleStakedSui
    let existing_fss = if !fss_refs.is_empty() {
        let first = builder.obj(ObjectArg::ImmOrOwnedObject(fss_refs[0]))?;
        for fss_ref in &fss_refs[1..] {
            let other = builder.obj(ObjectArg::ImmOrOwnedObject(*fss_ref))?;
            builder.command(Command::move_call(
                SUI_SYSTEM_PACKAGE_ID,
                Identifier::new("staking_pool")?,
                Identifier::new("join_fungible_staked_sui")?,
                vec![],
                vec![first, other],
            ));
        }
        Some(first)
    } else {
        None
    };

    // Phase 2: Convert each StakedSui → FSS
    let mut new_fss_results = Vec::new();
    for staked_ref in &staked_sui_refs {
        let staked_sui_arg = builder.obj(ObjectArg::ImmOrOwnedObject(*staked_ref))?;
        let result = builder.command(Command::move_call(
            SUI_SYSTEM_PACKAGE_ID,
            SUI_SYSTEM_MODULE_NAME.to_owned(),
            Identifier::new("convert_to_fungible_staked_sui")?,
            vec![],
            vec![system_state, staked_sui_arg],
        ));
        new_fss_results.push(result);
    }

    // Phase 3: Merge all new FSS together using join_fungible_staked_sui
    if new_fss_results.len() > 1 {
        for i in 1..new_fss_results.len() {
            builder.command(Command::move_call(
                SUI_SYSTEM_PACKAGE_ID,
                Identifier::new("staking_pool")?,
                Identifier::new("join_fungible_staked_sui")?,
                vec![],
                vec![new_fss_results[0], new_fss_results[i]],
            ));
        }
    }

    // Phase 4: Merge into existing or transfer to sender
    if let Some(existing) = existing_fss {
        if !new_fss_results.is_empty() {
            builder.command(Command::move_call(
                SUI_SYSTEM_PACKAGE_ID,
                Identifier::new("staking_pool")?,
                Identifier::new("join_fungible_staked_sui")?,
                vec![],
                vec![existing, new_fss_results[0]],
            ));
        }
        // existing FSS is already owned by sender, no transfer needed
    } else if !new_fss_results.is_empty() {
        let sender_arg = builder.pure(sender)?;
        builder.command(Command::TransferObjects(
            vec![new_fss_results[0]],
            sender_arg,
        ));
    }

    Ok(builder.finish())
}
