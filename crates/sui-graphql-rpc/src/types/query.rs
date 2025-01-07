// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use async_graphql::{connection::Connection, *};
use fastcrypto::encoding::{Base64, Encoding};
use move_core_types::account_address::AccountAddress;
use serde::de::DeserializeOwned;
use sui_json_rpc_types::DevInspectArgs;
use sui_sdk::SuiClient;
use sui_types::transaction::{TransactionData, TransactionKind};
use sui_types::{gas_coin::GAS, transaction::TransactionDataAPI, TypeTag};

use super::move_package::{
    self, MovePackage, MovePackageCheckpointFilter, MovePackageVersionFilter,
};
use super::move_registry::named_move_package::NamedMovePackage;
use super::move_registry::named_type::NamedType;
use super::object::ObjectKey;
use super::suins_registration::NameService;
use super::uint53::UInt53;
use super::{
    address::Address,
    available_range::AvailableRange,
    chain_identifier::ChainIdentifier,
    checkpoint::{self, Checkpoint, CheckpointId},
    coin::Coin,
    coin_metadata::CoinMetadata,
    cursor::Page,
    digest::Digest,
    dry_run_result::DryRunResult,
    epoch::{self, Epoch},
    event::{self, Event, EventFilter},
    move_type::MoveType,
    object::{self, Object, ObjectFilter},
    owner::Owner,
    protocol_config::ProtocolConfigs,
    sui_address::SuiAddress,
    suins_registration::Domain,
    transaction_block::{self, TransactionBlock, TransactionBlockFilter},
    transaction_metadata::TransactionMetadata,
    type_filter::ExactTypeFilter,
};
use crate::connection::ScanConnection;
use crate::server::watermark_task::Watermark;
use crate::types::base64::Base64 as GraphQLBase64;
use crate::types::zklogin_verify_signature::verify_zklogin_signature;
use crate::types::zklogin_verify_signature::ZkLoginIntentScope;
use crate::types::zklogin_verify_signature::ZkLoginVerifyResult;
use crate::{config::ServiceConfig, error::Error, mutation::Mutation};

pub(crate) struct Query;
pub(crate) type SuiGraphQLSchema = async_graphql::Schema<Query, Mutation, EmptySubscription>;

#[Object]
impl Query {
    /// First four bytes of the network's genesis checkpoint digest (uniquely identifies the
    /// network).
    async fn chain_identifier(&self, ctx: &Context<'_>) -> Result<String> {
        // we want to panic if the chain identifier is missing, as there's something wrong with
        // the service.
        let chain_id: ChainIdentifier = *ctx.data_unchecked();

        if let Some(id) = chain_id.0 {
            Ok(id.to_string())
        } else {
            Err(Error::Internal(
                "Chain identifier not initialized.".to_string(),
            ))
            .extend()
        }
    }

    /// Range of checkpoints that the RPC has data available for (for data
    /// that can be tied to a particular checkpoint).
    async fn available_range(&self, ctx: &Context<'_>) -> Result<AvailableRange> {
        let Watermark { hi_cp, .. } = *ctx.data()?;
        AvailableRange::query(ctx.data_unchecked(), hi_cp)
            .await
            .extend()
    }

    /// Configuration for this RPC service
    async fn service_config(&self, ctx: &Context<'_>) -> Result<ServiceConfig> {
        ctx.data()
            .map_err(|_| Error::Internal("Unable to fetch service configuration.".to_string()))
            .cloned()
            .extend()
    }

    /// Simulate running a transaction to inspect its effects without
    /// committing to them on-chain.
    ///
    /// `txBytes` either a `TransactionData` struct or a `TransactionKind`
    ///     struct, BCS-encoded and then Base64-encoded.  The expected
    ///     type is controlled by the presence or absence of `txMeta`: If
    ///     present, `txBytes` is assumed to be a `TransactionKind`, if
    ///     absent, then `TransactionData`.
    ///
    /// `txMeta` the data that is missing from a `TransactionKind` to make
    ///     a `TransactionData` (sender address and gas information).  All
    ///     its fields are nullable.
    ///
    /// `skipChecks` optional flag to disable the usual verification
    ///     checks that prevent access to objects that are owned by
    ///     addresses other than the sender, and calling non-public,
    ///     non-entry functions, and some other checks.  Defaults to false.
    async fn dry_run_transaction_block(
        &self,
        ctx: &Context<'_>,
        tx_bytes: String,
        tx_meta: Option<TransactionMetadata>,
        skip_checks: Option<bool>,
    ) -> Result<DryRunResult> {
        let skip_checks = skip_checks.unwrap_or(false);

        let sui_sdk_client: &Option<SuiClient> = ctx
            .data()
            .map_err(|_| Error::Internal("Unable to fetch Sui SDK client".to_string()))
            .extend()?;
        let sui_sdk_client = sui_sdk_client
            .as_ref()
            .ok_or_else(|| Error::Internal("Sui SDK client not initialized".to_string()))
            .extend()?;

        let (sender_address, tx_kind, gas_price, gas_sponsor, gas_budget, gas_objects) =
            if let Some(TransactionMetadata {
                sender,
                gas_price,
                gas_objects,
                gas_budget,
                gas_sponsor,
            }) = tx_meta
            {
                // This implies `TransactionKind`
                let tx_kind = deserialize_tx_data::<TransactionKind>(&tx_bytes)?;

                // Default is 0x0
                let sender_address = sender.unwrap_or_else(|| AccountAddress::ZERO.into()).into();

                let gas_sponsor = gas_sponsor.map(|addr| addr.into());

                let gas_objects = gas_objects.map(|objs| {
                    objs.into_iter()
                        .map(|obj| (obj.address.into(), obj.version.into(), obj.digest.into()))
                        .collect()
                });

                (
                    sender_address,
                    tx_kind,
                    gas_price.map(|p| p.into()),
                    gas_sponsor,
                    gas_budget.map(|b| b.into()),
                    gas_objects,
                )
            } else {
                // This implies `TransactionData`
                let tx_data = deserialize_tx_data::<TransactionData>(&tx_bytes)?;

                (
                    tx_data.sender(),
                    tx_data.clone().into_kind(),
                    Some(tx_data.gas_price().into()),
                    Some(tx_data.gas_owner()),
                    Some(tx_data.gas_budget().into()),
                    Some(tx_data.gas().to_vec()),
                )
            };

        let dev_inspect_args = DevInspectArgs {
            gas_sponsor,
            gas_budget,
            gas_objects,
            show_raw_txn_data_and_effects: Some(true),
            skip_checks: Some(skip_checks),
        };

        let res = sui_sdk_client
            .read_api()
            .dev_inspect_transaction_block(
                sender_address,
                tx_kind,
                gas_price,
                None,
                Some(dev_inspect_args),
            )
            .await?;

        DryRunResult::try_from(res).extend()
    }

    /// Look up an Owner by its SuiAddress.
    ///
    /// `rootVersion` represents the version of the root object in some nested chain of dynamic
    /// fields. It allows consistent historical queries for the case of wrapped objects, which don't
    /// have a version. For example, if querying the dynamic field of a table wrapped in a parent
    /// object, passing the parent object's version here will ensure we get the dynamic field's
    /// state at the moment that parent's version was created.
    ///
    /// Also, if this Owner is an object itself, `rootVersion` will be used to bound its version
    /// from above when querying `Owner.asObject`. This can be used, for example, to get the
    /// contents of a dynamic object field when its parent was at `rootVersion`.
    ///
    /// If `rootVersion` is omitted, dynamic fields will be from a consistent snapshot of the Sui
    /// state at the latest checkpoint known to the GraphQL RPC. Similarly, `Owner.asObject` will
    /// return the object's version at the latest checkpoint.
    async fn owner(
        &self,
        ctx: &Context<'_>,
        address: SuiAddress,
        root_version: Option<UInt53>,
    ) -> Result<Option<Owner>> {
        let Watermark { hi_cp, .. } = *ctx.data()?;
        Ok(Some(Owner {
            address,
            checkpoint_viewed_at: hi_cp,
            root_version: root_version.map(|v| v.into()),
        }))
    }

    /// The object corresponding to the given address at the (optionally) given version.
    /// When no version is given, the latest version is returned.
    async fn object(
        &self,
        ctx: &Context<'_>,
        address: SuiAddress,
        version: Option<UInt53>,
    ) -> Result<Option<Object>> {
        let Watermark { hi_cp, .. } = *ctx.data()?;
        let key = match version {
            Some(version) => Object::at_version(version.into(), hi_cp),
            None => Object::latest_at(hi_cp),
        };

        Object::query(ctx, address, key).await.extend()
    }

    /// The package corresponding to the given address (at the optionally given version).
    ///
    /// When no version is given, the package is loaded directly from the address given. Otherwise,
    /// the address is translated before loading to point to the package whose original ID matches
    /// the package at `address`, but whose version is `version`. For non-system packages, this
    /// might result in a different address than `address` because different versions of a package,
    /// introduced by upgrades, exist at distinct addresses.
    ///
    /// Note that this interpretation of `version` is different from a historical object read (the
    /// interpretation of `version` for the `object` query).
    async fn package(
        &self,
        ctx: &Context<'_>,
        address: SuiAddress,
        version: Option<UInt53>,
    ) -> Result<Option<MovePackage>> {
        let Watermark { hi_cp, .. } = *ctx.data()?;
        let key = match version {
            Some(version) => MovePackage::by_version(version.into(), hi_cp),
            None => MovePackage::by_id_at(hi_cp),
        };

        MovePackage::query(ctx, address, key).await.extend()
    }

    /// The latest version of the package at `address`.
    ///
    /// This corresponds to the package with the highest `version` that shares its original ID with
    /// the package at `address`.
    async fn latest_package(
        &self,
        ctx: &Context<'_>,
        address: SuiAddress,
    ) -> Result<Option<MovePackage>> {
        let Watermark { hi_cp, .. } = *ctx.data()?;
        MovePackage::query(ctx, address, MovePackage::latest_at(hi_cp))
            .await
            .extend()
    }

    /// Look-up an Account by its SuiAddress.
    async fn address(&self, ctx: &Context<'_>, address: SuiAddress) -> Result<Option<Address>> {
        let Watermark { hi_cp, .. } = *ctx.data()?;

        Ok(Some(Address {
            address,
            checkpoint_viewed_at: hi_cp,
        }))
    }

    /// Fetch a structured representation of a concrete type, including its layout information.
    /// Fails if the type is malformed.
    async fn type_(&self, type_: String) -> Result<MoveType> {
        Ok(TypeTag::from_str(&type_)
            .map_err(|e| Error::Client(format!("Bad type: {e}")))
            .extend()?
            .into())
    }

    /// Fetch epoch information by ID (defaults to the latest epoch).
    async fn epoch(&self, ctx: &Context<'_>, id: Option<UInt53>) -> Result<Option<Epoch>> {
        let Watermark { hi_cp, .. } = *ctx.data()?;
        Epoch::query(ctx, id.map(|id| id.into()), hi_cp)
            .await
            .extend()
    }

    /// Fetch checkpoint information by sequence number or digest (defaults to the latest available
    /// checkpoint).
    async fn checkpoint(
        &self,
        ctx: &Context<'_>,
        id: Option<CheckpointId>,
    ) -> Result<Option<Checkpoint>> {
        let Watermark { hi_cp, .. } = *ctx.data()?;
        Checkpoint::query(ctx, id.unwrap_or_default(), hi_cp)
            .await
            .extend()
    }

    /// Fetch a transaction block by its transaction digest.
    async fn transaction_block(
        &self,
        ctx: &Context<'_>,
        digest: Digest,
    ) -> Result<Option<TransactionBlock>> {
        let Watermark { hi_cp, .. } = *ctx.data()?;
        let lookup = TransactionBlock::by_digest(digest, hi_cp);
        TransactionBlock::query(ctx, lookup).await.extend()
    }

    /// Fetch a list of objects by their IDs and versions.
    async fn multi_get_objects(
        &self,
        ctx: &Context<'_>,
        keys: Vec<ObjectKey>,
    ) -> Result<Vec<Object>> {
        let cfg: &ServiceConfig = ctx.data_unchecked();
        if keys.len() > cfg.limits.max_multi_get_objects_keys as usize {
            return Err(Error::Client(format!(
                "Number of keys exceeds max limit of '{}'",
                cfg.limits.max_multi_get_objects_keys
            ))
            .into());
        }

        let Watermark { hi_cp, .. } = *ctx.data()?;

        Object::query_many(ctx, keys, hi_cp).await.extend()
    }

    /// The coin objects that exist in the network.
    ///
    /// The type field is a string of the inner type of the coin by which to filter (e.g.
    /// `0x2::sui::SUI`). If no type is provided, it will default to `0x2::sui::SUI`.
    async fn coins(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<object::Cursor>,
        last: Option<u64>,
        before: Option<object::Cursor>,
        type_: Option<ExactTypeFilter>,
    ) -> Result<Connection<String, Coin>> {
        let Watermark { hi_cp, .. } = *ctx.data()?;

        let page = Page::from_params(ctx.data_unchecked(), first, after, last, before)?;
        let coin = type_.map_or_else(GAS::type_tag, |t| t.0);
        Coin::paginate(
            ctx.data_unchecked(),
            page,
            coin,
            /* owner */ None,
            hi_cp,
        )
        .await
        .extend()
    }

    // The epochs of the network
    async fn epochs(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<epoch::Cursor>,
        last: Option<u64>,
        before: Option<epoch::Cursor>,
    ) -> Result<Connection<String, Epoch>> {
        let Watermark { hi_cp, .. } = *ctx.data()?;

        let page = Page::from_params(ctx.data_unchecked(), first, after, last, before)?;
        Epoch::paginate(ctx.data_unchecked(), page, hi_cp)
            .await
            .extend()
    }

    /// The checkpoints that exist in the network.
    async fn checkpoints(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<checkpoint::Cursor>,
        last: Option<u64>,
        before: Option<checkpoint::Cursor>,
    ) -> Result<Connection<String, Checkpoint>> {
        let Watermark { hi_cp, .. } = *ctx.data()?;

        let page = Page::from_params(ctx.data_unchecked(), first, after, last, before)?;
        Checkpoint::paginate(ctx.data_unchecked(), page, /* epoch */ None, hi_cp)
            .await
            .extend()
    }

    /// The transaction blocks that exist in the network.
    ///
    /// `scanLimit` restricts the number of candidate transactions scanned when gathering a page of
    /// results. It is required for queries that apply more than two complex filters (on function,
    /// kind, sender, recipient, input object, changed object, or ids), and can be at most
    /// `serviceConfig.maxScanLimit`.
    ///
    /// When the scan limit is reached the page will be returned even if it has fewer than `first`
    /// results when paginating forward (`last` when paginating backwards). If there are more
    /// transactions to scan, `pageInfo.hasNextPage` (or `pageInfo.hasPreviousPage`) will be set to
    /// `true`, and `PageInfo.endCursor` (or `PageInfo.startCursor`) will be set to the last
    /// transaction that was scanned as opposed to the last (or first) transaction in the page.
    ///
    /// Requesting the next (or previous) page after this cursor will resume the search, scanning
    /// the next `scanLimit` many transactions in the direction of pagination, and so on until all
    /// transactions in the scanning range have been visited.
    ///
    /// By default, the scanning range includes all transactions known to GraphQL, but it can be
    /// restricted by the `after` and `before` cursors, and the `beforeCheckpoint`,
    /// `afterCheckpoint` and `atCheckpoint` filters.
    async fn transaction_blocks(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<transaction_block::Cursor>,
        last: Option<u64>,
        before: Option<transaction_block::Cursor>,
        filter: Option<TransactionBlockFilter>,
        scan_limit: Option<u64>,
    ) -> Result<ScanConnection<String, TransactionBlock>> {
        let Watermark { hi_cp, .. } = *ctx.data()?;

        let page = Page::from_params(ctx.data_unchecked(), first, after, last, before)?;

        TransactionBlock::paginate(ctx, page, filter.unwrap_or_default(), hi_cp, scan_limit)
            .await
            .extend()
    }

    /// Query events that are emitted in the network.
    /// We currently do not support filtering by emitting module and event type
    /// at the same time so if both are provided in one filter, the query will error.
    async fn events(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<event::Cursor>,
        last: Option<u64>,
        before: Option<event::Cursor>,
        filter: Option<EventFilter>,
    ) -> Result<Connection<String, Event>> {
        let Watermark { hi_cp, .. } = *ctx.data()?;

        let page = Page::from_params(ctx.data_unchecked(), first, after, last, before)?;
        Event::paginate(
            ctx.data_unchecked(),
            page,
            filter.unwrap_or_default(),
            hi_cp,
        )
        .await
        .extend()
    }

    /// The objects that exist in the network.
    async fn objects(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<object::Cursor>,
        last: Option<u64>,
        before: Option<object::Cursor>,
        filter: Option<ObjectFilter>,
    ) -> Result<Connection<String, Object>> {
        let Watermark { hi_cp, .. } = *ctx.data()?;

        let page = Page::from_params(ctx.data_unchecked(), first, after, last, before)?;
        Object::paginate(
            ctx.data_unchecked(),
            page,
            filter.unwrap_or_default(),
            hi_cp,
        )
        .await
        .extend()
    }

    /// The Move packages that exist in the network, optionally filtered to be strictly before
    /// `beforeCheckpoint` and/or strictly after `afterCheckpoint`.
    ///
    /// This query returns all versions of a given user package that appear between the specified
    /// checkpoints, but only records the latest versions of system packages.
    async fn packages(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<move_package::Cursor>,
        last: Option<u64>,
        before: Option<move_package::Cursor>,
        filter: Option<MovePackageCheckpointFilter>,
    ) -> Result<Connection<String, MovePackage>> {
        let Watermark { hi_cp, .. } = *ctx.data()?;

        let page = Page::from_params(ctx.data_unchecked(), first, after, last, before)?;
        MovePackage::paginate_by_checkpoint(ctx.data_unchecked(), page, filter, hi_cp)
            .await
            .extend()
    }

    /// Fetch all versions of package at `address` (packages that share this package's original ID),
    /// optionally bounding the versions exclusively from below with `afterVersion`, or from above
    /// with `beforeVersion`.
    async fn package_versions(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<move_package::Cursor>,
        last: Option<u64>,
        before: Option<move_package::Cursor>,
        address: SuiAddress,
        filter: Option<MovePackageVersionFilter>,
    ) -> Result<Connection<String, MovePackage>> {
        let Watermark { hi_cp, .. } = *ctx.data()?;

        let page = Page::from_params(ctx.data_unchecked(), first, after, last, before)?;
        MovePackage::paginate_by_version(ctx.data_unchecked(), page, address, filter, hi_cp)
            .await
            .extend()
    }

    /// Fetch the protocol config by protocol version (defaults to the latest protocol
    /// version known to the GraphQL service).
    async fn protocol_config(
        &self,
        ctx: &Context<'_>,
        protocol_version: Option<UInt53>,
    ) -> Result<ProtocolConfigs> {
        ProtocolConfigs::query(ctx.data_unchecked(), protocol_version.map(|v| v.into()))
            .await
            .extend()
    }

    /// Resolves a SuiNS `domain` name to an address, if it has been bound.
    async fn resolve_suins_address(
        &self,
        ctx: &Context<'_>,
        domain: Domain,
    ) -> Result<Option<Address>> {
        let Watermark { hi_cp, .. } = *ctx.data()?;
        Ok(NameService::resolve_to_record(ctx, &domain, hi_cp)
            .await
            .extend()?
            .and_then(|r| r.target_address)
            .map(|a| Address {
                address: a.into(),
                checkpoint_viewed_at: hi_cp,
            }))
    }

    /// Fetch a package by its name (using dot move service)
    async fn package_by_name(
        &self,
        ctx: &Context<'_>,
        name: String,
    ) -> Result<Option<MovePackage>> {
        let Watermark { hi_cp, .. } = *ctx.data()?;

        NamedMovePackage::query(ctx, &name, hi_cp).await.extend()
    }

    /// Fetch a type that includes dot move service names in it.
    async fn type_by_name(&self, ctx: &Context<'_>, name: String) -> Result<MoveType> {
        let Watermark { hi_cp, .. } = *ctx.data()?;
        let type_tag = NamedType::query(ctx, &name, hi_cp).await?;

        Ok(type_tag.into())
    }

    /// The coin metadata associated with the given coin type. Note that if the latest version of
    /// the coin's metadata is wrapped or deleted, it will not be found.
    async fn coin_metadata(
        &self,
        ctx: &Context<'_>,
        coin_type: ExactTypeFilter,
    ) -> Result<Option<CoinMetadata>> {
        let Watermark { hi_cp, .. } = *ctx.data()?;
        CoinMetadata::query(ctx.data_unchecked(), coin_type.0, hi_cp)
            .await
            .extend()
    }

    /// Verify a zkLogin signature based on the provided transaction or personal message
    /// based on current epoch, chain id, and latest JWKs fetched on-chain. If the
    /// signature is valid, the function returns a `ZkLoginVerifyResult` with success as
    /// true and an empty list of errors. If the signature is invalid, the function returns
    /// a `ZkLoginVerifyResult` with success as false with a list of errors.
    ///
    /// - `bytes` is either the personal message in raw bytes or transaction data bytes in
    ///    BCS-encoded and then Base64-encoded.
    /// - `signature` is a serialized zkLogin signature that is Base64-encoded.
    /// - `intentScope` is an enum that specifies the intent scope to be used to parse bytes.
    /// - `author` is the address of the signer of the transaction or personal msg.
    async fn verify_zklogin_signature(
        &self,
        ctx: &Context<'_>,
        bytes: GraphQLBase64,
        signature: GraphQLBase64,
        intent_scope: ZkLoginIntentScope,
        author: SuiAddress,
    ) -> Result<ZkLoginVerifyResult> {
        verify_zklogin_signature(ctx, bytes, signature, intent_scope, author)
            .await
            .extend()
    }
}

fn deserialize_tx_data<T>(tx_bytes: &str) -> Result<T>
where
    T: DeserializeOwned,
{
    bcs::from_bytes(
        &Base64::decode(tx_bytes)
            .map_err(|e| {
                Error::Client(format!(
                    "Unable to deserialize transaction bytes from Base64: {e}"
                ))
            })
            .extend()?,
    )
    .map_err(|e| {
        Error::Client(format!(
            "Unable to deserialize transaction bytes as BCS: {e}"
        ))
    })
    .extend()
}
