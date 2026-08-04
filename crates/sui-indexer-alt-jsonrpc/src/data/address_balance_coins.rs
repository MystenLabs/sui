// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context as _;
use anyhow::ensure;
use move_core_types::language_storage::TypeTag;
use sui_json_rpc_types::Coin;
use sui_json_rpc_types::SuiObjectData;
use sui_json_rpc_types::SuiObjectDataOptions;
use sui_types::accumulator_root::AccumulatorKey;
use sui_types::accumulator_root::AccumulatorValue;
use sui_types::balance::Balance;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SuiAddress;
use sui_types::coin_reservation;
use sui_types::coin_reservation::ParsedObjectRefWithdrawal;
use sui_types::committee::EpochId;
use sui_types::digests::ChainIdentifier;
use sui_types::digests::ObjectDigest;
use sui_types::object::MoveObject;
use sui_types::object::Object;
use sui_types::object::Owner;

use crate::api::objects::response::object_data_with_options;
use crate::context::Context;
use crate::data::load_live;
use crate::error::RpcError;

/// Struct to support a view of an address balance as a coin.
///
/// An address balance is an entry folded into a balance accumulator. However, consumers of
/// jsonrpc-alt expect `ObjectRef`s, so the implementation spoofs one using a masked ID, the
/// accumulator object's version, and the digest field repurposed as a carrier for (epoch, amount).
pub(crate) struct AddressBalanceCoin {
    /// Fabricated `Coin` object to render the balance as if it were a coin. Its ID and version are
    /// already the masked ID and the accumulator object's version.
    contents: Object,
    /// Spoofed digest.
    digest: ObjectDigest,
}

impl AddressBalanceCoin {
    /// Synthesize the address balance coin for the given owner and coin type.
    ///
    /// Returns `None` if the accumulator object doesn't exist or its balance is zero.
    pub(crate) async fn by_owner(
        ctx: &Context,
        owner: SuiAddress,
        coin_type: TypeTag,
    ) -> Result<Option<Self>, anyhow::Error> {
        let chain_identifier = ctx
            .chain_identifier()
            .context("Chain identifier not available (no database configured)")?;

        let balance_type = Balance::type_tag(coin_type);
        let accumulator_id = AccumulatorValue::get_field_id(owner, &balance_type)
            .context("Failed to derive accumulator field ID")?;

        Self::load(ctx, *accumulator_id.inner(), chain_identifier).await
    }

    /// Try to resolve an object ID as a masked address balance coin.
    pub(crate) async fn by_object_id(
        ctx: &Context,
        object_id: ObjectID,
    ) -> Result<Option<Self>, anyhow::Error> {
        let Some(chain_identifier) = ctx.chain_identifier() else {
            return Ok(None);
        };
        let unmasked_id = coin_reservation::mask_or_unmask_id(object_id, chain_identifier);

        let Some(coin) = Self::load(ctx, unmasked_id, chain_identifier).await? else {
            return Ok(None);
        };

        // `encode` re-derives the masked ID; masking is a XOR involution, so it must round-trip
        // back to the ID the caller asked about.
        ensure!(
            coin.contents.id() == object_id,
            "Masked address balance coin ID did not round-trip",
        );

        Ok(Some(coin))
    }

    /// Project into the `Coin` shape returned by the coin API.
    pub(crate) fn into_coin(self) -> Result<Coin, anyhow::Error> {
        let coin_type = self
            .contents
            .coin_type_maybe()
            .context("Fabricated address balance object is not a coin")?;
        let balance = self
            .contents
            .as_coin_maybe()
            .context("Fabricated address balance object is not a coin")?
            .balance
            .value();

        Ok(Coin {
            coin_type: coin_type.to_canonical_string(/* with_prefix */ true),
            coin_object_id: self.contents.id(),
            version: self.contents.version(),
            digest: self.digest,
            balance,
            previous_transaction: self.contents.previous_transaction,
        })
    }

    /// Project into the `SuiObjectData` shape returned by the object APIs.
    pub(crate) async fn into_sui_object_data(
        self,
        ctx: &Context,
        options: &SuiObjectDataOptions,
    ) -> Result<SuiObjectData, RpcError> {
        let digest = self.digest;
        let mut data = object_data_with_options(ctx, self.contents, options).await?;
        data.digest = digest;
        Ok(data)
    }

    /// Load the live accumulator object and synthesize the coin from it.
    async fn load(
        ctx: &Context,
        accumulator_id: ObjectID,
        chain_identifier: ChainIdentifier,
    ) -> Result<Option<Self>, anyhow::Error> {
        if !super::latest_feature_flag(ctx, "enable_coin_reservation_obj_refs").await? {
            return Ok(None);
        }

        let (accumulator_obj, epoch) =
            tokio::try_join!(load_live(ctx, accumulator_id), super::latest_epoch(ctx),)?;

        let Some(accumulator_obj) = accumulator_obj else {
            return Ok(None);
        };

        Ok(Self::from_accumulator_object(
            accumulator_obj,
            epoch,
            chain_identifier,
        ))
    }

    /// Construct from a balance accumulator object.
    ///
    /// Returns `None` if the object is not a balance accumulator field or its balance is zero.
    fn from_accumulator_object(
        accumulator_obj: Object,
        epoch: EpochId,
        chain_identifier: ChainIdentifier,
    ) -> Option<Self> {
        let move_object = accumulator_obj.data.try_as_move()?;
        let currency_type = move_object.type_().balance_accumulator_field_type_maybe()?;

        let accumulator_id = accumulator_obj.id();
        let accumulator_version = accumulator_obj.version();

        let (AccumulatorKey { owner }, value) = move_object.try_into().ok()?;

        let balance = value.as_u128().map(|v| v as u64).unwrap_or(0);
        if balance == 0 {
            return None;
        }

        let (masked_id, _, digest) = ParsedObjectRefWithdrawal::new(accumulator_id, epoch, balance)
            .encode(accumulator_version, chain_identifier);

        let contents = Object::new_move(
            MoveObject::new_coin(currency_type, accumulator_version, masked_id, balance),
            Owner::AddressOwner(owner),
            accumulator_obj.previous_transaction,
        );

        Some(Self { contents, digest })
    }
}
