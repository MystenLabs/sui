// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context as _;
use move_core_types::language_storage::TypeTag;
use sui_json_rpc_types::Coin;
use sui_types::accumulator_root::AccumulatorKey;
use sui_types::accumulator_root::AccumulatorValue;
use sui_types::balance::Balance;
use sui_types::base_types::ObjectID;
use sui_types::base_types::ObjectRef;
use sui_types::base_types::SuiAddress;
use sui_types::coin_reservation;
use sui_types::coin_reservation::ParsedObjectRefWithdrawal;
use sui_types::object::MoveObject;
use sui_types::object::Object;
use sui_types::object::Owner;

use crate::context::Context;
use crate::data::load_live;

/// Synthesize an address balance coin for the given owner and coin type.
///
/// Returns `None` if the accumulator object doesn't exist (e.g. the address has never received
/// this coin type as an address balance).
pub(crate) async fn load_address_balance_coin(
    ctx: &Context,
    owner: SuiAddress,
    coin_type: TypeTag,
    address_balance: u64,
) -> Result<Option<Coin>, anyhow::Error> {
    if address_balance == 0 {
        return Ok(None);
    }

    let balance_type = Balance::type_tag(coin_type.clone());
    let accumulator_id = AccumulatorValue::get_field_id(owner, &balance_type)
        .context("Failed to derive accumulator field ID")?;

    let Some(accumulator_obj) = load_live(ctx, *accumulator_id.inner()).await? else {
        // No accumulator field exists for this owner and coin type.
        return Ok(None);
    };

    let accumulator_version = accumulator_obj.version();
    let previous_transaction = accumulator_obj.previous_transaction;
    let epoch = super::current_epoch(ctx).await?;
    let chain_identifier = ctx
        .chain_identifier()
        .context("Chain identifier not available (no database configured)")?;

    let object_ref: ObjectRef =
        ParsedObjectRefWithdrawal::new(*accumulator_id.inner(), epoch, address_balance)
            .encode(accumulator_version, chain_identifier);

    let masked_id = object_ref.0;

    let coin = Coin {
        coin_type: coin_type.to_canonical_string(/* with_prefix */ true),
        coin_object_id: masked_id,
        version: object_ref.1,
        digest: object_ref.2,
        balance: address_balance,
        previous_transaction,
    };

    Ok(Some(coin))
}

/// Try to resolve an object ID as a masked address balance coin.
///
/// When `getObject` is called with an ID that doesn't exist, this function unmaskes the ID
/// (XOR with chain identifier) and checks if it points to a balance accumulator object.
/// If so, it synthesizes a Coin object to return to the client.
pub(crate) async fn try_resolve_address_balance_object(
    ctx: &Context,
    object_id: ObjectID,
) -> Result<Option<Object>, anyhow::Error> {
    let Some(chain_identifier) = ctx.chain_identifier() else {
        return Ok(None);
    };
    let unmasked_id = coin_reservation::mask_or_unmask_id(object_id, chain_identifier);

    let Some(accumulator_obj) = load_live(ctx, unmasked_id).await? else {
        return Ok(None);
    };

    let Some(move_object) = accumulator_obj.data.try_as_move() else {
        return Ok(None);
    };

    let Some(currency_type) = move_object.type_().balance_accumulator_field_type_maybe() else {
        return Ok(None);
    };

    let accumulator_version = accumulator_obj.version();

    let Ok((AccumulatorKey { owner }, value)) = move_object.try_into() else {
        return Ok(None);
    };

    let balance = value.as_u128().map(|v| v as u64).unwrap_or(0);
    if balance == 0 {
        return Ok(None);
    }

    let coin_object = Object::new_move(
        MoveObject::new_coin(currency_type, accumulator_version, object_id, balance),
        Owner::AddressOwner(owner),
        accumulator_obj.previous_transaction,
    );

    Ok(Some(coin_object))
}
