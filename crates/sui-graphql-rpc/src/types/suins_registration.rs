// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use super::{
    available_range::AvailableRange,
    balance::{self, Balance},
    base64::Base64,
    big_int::BigInt,
    checkpoint::Checkpoint,
    coin::Coin,
    cursor::Page,
    display::DisplayEntry,
    dynamic_field::{DynamicField, DynamicFieldName},
    move_object::{MoveObject, MoveObjectImpl},
    move_value::MoveValue,
    object::{self, Object, ObjectFilter, ObjectImpl, ObjectOwner, ObjectStatus},
    owner::OwnerImpl,
    stake::StakedSui,
    string_input::impl_string_input,
    sui_address::SuiAddress,
    transaction_block::{self, TransactionBlock, TransactionBlockFilter},
    type_filter::ExactTypeFilter,
    uint53::UInt53,
};
use crate::{
    connection::ScanConnection,
    consistency::{build_objects_query, View},
    data::{Db, DbConnection, QueryExecutor},
    error::Error,
};
use async_graphql::{connection::Connection, *};
use diesel_async::scoped_futures::ScopedFutureExt;
use move_core_types::{ident_str, identifier::IdentStr, language_storage::StructTag};
use serde::{Deserialize, Serialize};
use sui_indexer::models::objects::StoredHistoryObject;
use sui_name_service::{Domain as NativeDomain, NameRecord, NameServiceConfig, NameServiceError};
use sui_types::{base_types::SuiAddress as NativeSuiAddress, dynamic_field::Field, id::UID};

const MOD_REGISTRATION: &IdentStr = ident_str!("suins_registration");
const TYP_REGISTRATION: &IdentStr = ident_str!("SuinsRegistration");

/// Represents the "core" of the name service (e.g. the on-chain registry and reverse registry). It
/// doesn't contain any fields because we look them up based on the `NameServiceConfig`.
pub(crate) struct NameService;

/// Wrap SuiNS Domain type to expose as a string scalar in GraphQL.
#[derive(Debug)]
pub(crate) struct Domain(NativeDomain);

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
#[graphql(remote = "sui_name_service::DomainFormat")]
pub enum DomainFormat {
    At,
    Dot,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct NativeSuinsRegistration {
    pub id: UID,
    pub domain: NativeDomain,
    pub domain_name: String,
    pub expiration_timestamp_ms: u64,
    pub image_url: String,
}

#[derive(Clone)]
pub(crate) struct SuinsRegistration {
    /// Representation of this SuinsRegistration as a generic Move object.
    pub super_: MoveObject,

    /// The deserialized representation of the Move object's contents.
    pub native: NativeSuinsRegistration,
}

/// Represents the results of a query for a domain's `NameRecord` and its parent's `NameRecord`. The
/// `expiration_timestamp_ms` on the name records are compared to the checkpoint's timestamp to
/// check that the domain is not expired.
pub(crate) struct DomainExpiration {
    /// The domain's `NameRecord`.
    pub name_record: Option<NameRecord>,
    /// The parent's `NameRecord`, populated only if the domain is a subdomain.
    pub parent_name_record: Option<NameRecord>,
    /// The timestamp of the checkpoint at which the query was made. This is used to check if the
    /// `expiration_timestamp_ms` on the name records are expired.
    pub checkpoint_timestamp_ms: u64,
}

pub(crate) enum SuinsRegistrationDowncastError {
    NotASuinsRegistration,
    Bcs(bcs::Error),
}

#[Object]
impl SuinsRegistration {
    pub(crate) async fn address(&self) -> SuiAddress {
        OwnerImpl::from(&self.super_.super_).address().await
    }

    /// Objects owned by this object, optionally `filter`-ed.
    pub(crate) async fn objects(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<object::Cursor>,
        last: Option<u64>,
        before: Option<object::Cursor>,
        filter: Option<ObjectFilter>,
    ) -> Result<Connection<String, MoveObject>> {
        OwnerImpl::from(&self.super_.super_)
            .objects(ctx, first, after, last, before, filter)
            .await
    }

    /// Total balance of all coins with marker type owned by this object. If type is not supplied,
    /// it defaults to `0x2::sui::SUI`.
    pub(crate) async fn balance(
        &self,
        ctx: &Context<'_>,
        type_: Option<ExactTypeFilter>,
    ) -> Result<Option<Balance>> {
        OwnerImpl::from(&self.super_.super_)
            .balance(ctx, type_)
            .await
    }

    /// The balances of all coin types owned by this object.
    pub(crate) async fn balances(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<balance::Cursor>,
        last: Option<u64>,
        before: Option<balance::Cursor>,
    ) -> Result<Connection<String, Balance>> {
        OwnerImpl::from(&self.super_.super_)
            .balances(ctx, first, after, last, before)
            .await
    }

    /// The coin objects for this object.
    ///
    ///`type` is a filter on the coin's type parameter, defaulting to `0x2::sui::SUI`.
    pub(crate) async fn coins(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<object::Cursor>,
        last: Option<u64>,
        before: Option<object::Cursor>,
        type_: Option<ExactTypeFilter>,
    ) -> Result<Connection<String, Coin>> {
        OwnerImpl::from(&self.super_.super_)
            .coins(ctx, first, after, last, before, type_)
            .await
    }

    /// The `0x3::staking_pool::StakedSui` objects owned by this object.
    pub(crate) async fn staked_suis(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<object::Cursor>,
        last: Option<u64>,
        before: Option<object::Cursor>,
    ) -> Result<Connection<String, StakedSui>> {
        OwnerImpl::from(&self.super_.super_)
            .staked_suis(ctx, first, after, last, before)
            .await
    }

    /// The domain explicitly configured as the default domain pointing to this object.
    pub(crate) async fn default_suins_name(
        &self,
        ctx: &Context<'_>,
        format: Option<DomainFormat>,
    ) -> Result<Option<String>> {
        OwnerImpl::from(&self.super_.super_)
            .default_suins_name(ctx, format)
            .await
    }

    /// The SuinsRegistration NFTs owned by this object. These grant the owner the capability to
    /// manage the associated domain.
    pub(crate) async fn suins_registrations(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<object::Cursor>,
        last: Option<u64>,
        before: Option<object::Cursor>,
    ) -> Result<Connection<String, SuinsRegistration>> {
        OwnerImpl::from(&self.super_.super_)
            .suins_registrations(ctx, first, after, last, before)
            .await
    }

    pub(crate) async fn version(&self) -> UInt53 {
        ObjectImpl(&self.super_.super_).version().await
    }

    /// The current status of the object as read from the off-chain store. The possible states are:
    /// NOT_INDEXED, the object is loaded from serialized data, such as the contents of a genesis or
    /// system package upgrade transaction. LIVE, the version returned is the most recent for the
    /// object, and it is not deleted or wrapped at that version. HISTORICAL, the object was
    /// referenced at a specific version or checkpoint, so is fetched from historical tables and may
    /// not be the latest version of the object. WRAPPED_OR_DELETED, the object is deleted or
    /// wrapped and only partial information can be loaded."
    pub(crate) async fn status(&self) -> ObjectStatus {
        ObjectImpl(&self.super_.super_).status().await
    }

    /// 32-byte hash that identifies the object's contents, encoded as a Base58 string.
    pub(crate) async fn digest(&self) -> Option<String> {
        ObjectImpl(&self.super_.super_).digest().await
    }

    /// The owner type of this object: Immutable, Shared, Parent, Address
    pub(crate) async fn owner(&self) -> Option<ObjectOwner> {
        ObjectImpl(&self.super_.super_).owner().await
    }

    /// The transaction block that created this version of the object.
    pub(crate) async fn previous_transaction_block(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<TransactionBlock>> {
        ObjectImpl(&self.super_.super_)
            .previous_transaction_block(ctx)
            .await
    }

    /// The amount of SUI we would rebate if this object gets deleted or mutated. This number is
    /// recalculated based on the present storage gas price.
    pub(crate) async fn storage_rebate(&self) -> Option<BigInt> {
        ObjectImpl(&self.super_.super_).storage_rebate().await
    }

    /// The transaction blocks that sent objects to this object.
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
    pub(crate) async fn received_transaction_blocks(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<transaction_block::Cursor>,
        last: Option<u64>,
        before: Option<transaction_block::Cursor>,
        filter: Option<TransactionBlockFilter>,
        scan_limit: Option<u64>,
    ) -> Result<ScanConnection<String, TransactionBlock>> {
        ObjectImpl(&self.super_.super_)
            .received_transaction_blocks(ctx, first, after, last, before, filter, scan_limit)
            .await
    }

    /// The Base64-encoded BCS serialization of the object's content.
    pub(crate) async fn bcs(&self) -> Result<Option<Base64>> {
        ObjectImpl(&self.super_.super_).bcs().await
    }

    /// Displays the contents of the Move object in a JSON string and through GraphQL types. Also
    /// provides the flat representation of the type signature, and the BCS of the corresponding
    /// data.
    pub(crate) async fn contents(&self) -> Option<MoveValue> {
        MoveObjectImpl(&self.super_).contents().await
    }

    /// Determines whether a transaction can transfer this object, using the TransferObjects
    /// transaction command or `sui::transfer::public_transfer`, both of which require the object to
    /// have the `key` and `store` abilities.
    pub(crate) async fn has_public_transfer(&self, ctx: &Context<'_>) -> Result<bool> {
        MoveObjectImpl(&self.super_).has_public_transfer(ctx).await
    }

    /// The set of named templates defined on-chain for the type of this object, to be handled
    /// off-chain. The server substitutes data from the object into these templates to generate a
    /// display string per template.
    pub(crate) async fn display(&self, ctx: &Context<'_>) -> Result<Option<Vec<DisplayEntry>>> {
        ObjectImpl(&self.super_.super_).display(ctx).await
    }

    /// Access a dynamic field on an object using its name. Names are arbitrary Move values whose
    /// type have `copy`, `drop`, and `store`, and are specified using their type, and their BCS
    /// contents, Base64 encoded.
    ///
    /// Dynamic fields on wrapped objects can be accessed by using the same API under the Owner
    /// type.
    pub(crate) async fn dynamic_field(
        &self,
        ctx: &Context<'_>,
        name: DynamicFieldName,
    ) -> Result<Option<DynamicField>> {
        OwnerImpl::from(&self.super_.super_)
            .dynamic_field(ctx, name, Some(self.super_.root_version()))
            .await
    }

    /// Access a dynamic object field on an object using its name. Names are arbitrary Move values
    /// whose type have `copy`, `drop`, and `store`, and are specified using their type, and their
    /// BCS contents, Base64 encoded. The value of a dynamic object field can also be accessed
    /// off-chain directly via its address (e.g. using `Query.object`).
    ///
    /// Dynamic fields on wrapped objects can be accessed by using the same API under the Owner
    /// type.
    pub(crate) async fn dynamic_object_field(
        &self,
        ctx: &Context<'_>,
        name: DynamicFieldName,
    ) -> Result<Option<DynamicField>> {
        OwnerImpl::from(&self.super_.super_)
            .dynamic_object_field(ctx, name, Some(self.super_.root_version()))
            .await
    }

    /// The dynamic fields and dynamic object fields on an object.
    ///
    /// Dynamic fields on wrapped objects can be accessed by using the same API under the Owner
    /// type.
    pub(crate) async fn dynamic_fields(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<object::Cursor>,
        last: Option<u64>,
        before: Option<object::Cursor>,
    ) -> Result<Connection<String, DynamicField>> {
        OwnerImpl::from(&self.super_.super_)
            .dynamic_fields(
                ctx,
                first,
                after,
                last,
                before,
                Some(self.super_.root_version()),
            )
            .await
    }

    /// Domain name of the SuinsRegistration object
    async fn domain(&self) -> &str {
        &self.native.domain_name
    }
}

impl NameService {
    /// Lookup the SuiNS NameRecord for the given `domain` name. `config` specifies where to find
    /// the domain name registry, and its type.
    ///
    /// `checkpoint_viewed_at` represents the checkpoint sequence number at which this was queried
    /// for.
    ///
    /// The `NameRecord` is returned only if it has not expired as of the `checkpoint_viewed_at` or
    /// latest checkpoint's timestamp.
    ///
    /// For leaf domains, the `NameRecord` is returned only if its parent is valid and not expired.
    pub(crate) async fn resolve_to_record(
        ctx: &Context<'_>,
        domain: &Domain,
        checkpoint_viewed_at: u64,
    ) -> Result<Option<NameRecord>, Error> {
        // Query for the domain's NameRecord and parent NameRecord if applicable. The checkpoint's
        // timestamp is also fetched. These values are used to determine if the domain is expired.
        let Some(domain_expiration) =
            Self::query_domain_expiration(ctx, domain, checkpoint_viewed_at).await?
        else {
            return Ok(None);
        };

        // Get the name_record from the query. If we didn't find it, we return as it means that the
        // requested name is not registered.
        let Some(name_record) = domain_expiration.name_record else {
            return Ok(None);
        };

        // If name record is SLD, or Node subdomain, we can check the expiration and return the
        // record if not expired.
        if !name_record.is_leaf_record() {
            return if !name_record.is_node_expired(domain_expiration.checkpoint_timestamp_ms) {
                Ok(Some(name_record))
            } else {
                Err(Error::NameService(NameServiceError::NameExpired))
            };
        }

        // If we cannot find the parent, then the name is expired.
        let Some(parent_name_record) = domain_expiration.parent_name_record else {
            return Err(Error::NameService(NameServiceError::NameExpired));
        };

        // If the parent is valid for this leaf, and not expired, then we can return the name
        // record. Otherwise, the name is expired.
        if parent_name_record.is_valid_leaf_parent(&name_record)
            && !parent_name_record.is_node_expired(domain_expiration.checkpoint_timestamp_ms)
        {
            Ok(Some(name_record))
        } else {
            Err(Error::NameService(NameServiceError::NameExpired))
        }
    }

    /// Lookup the SuiNS Domain for the given `address`. `config` specifies where to find the domain
    /// name registry, and its type.
    ///
    /// `checkpoint_viewed_at` represents the checkpoint sequence number at which this was queried
    /// for.
    pub(crate) async fn reverse_resolve_to_name(
        ctx: &Context<'_>,
        address: SuiAddress,
        checkpoint_viewed_at: u64,
    ) -> Result<Option<NativeDomain>, Error> {
        let config: &NameServiceConfig = ctx.data_unchecked();

        let reverse_record_id = config.reverse_record_field_id(address.as_slice());

        let Some(object) = MoveObject::query(
            ctx,
            reverse_record_id.into(),
            Object::latest_at(checkpoint_viewed_at),
        )
        .await?
        else {
            return Ok(None);
        };

        let field: Field<NativeSuiAddress, NativeDomain> = object
            .native
            .to_rust()
            .ok_or_else(|| Error::Internal("Malformed Suins Domain".to_string()))?;

        let domain = Domain(field.value);

        // We attempt to resolve the domain to a record, and if it fails, we return None. That way
        // we can validate that the name has not expired and is still valid.
        let Some(_) = Self::resolve_to_record(ctx, &domain, checkpoint_viewed_at).await? else {
            return Ok(None);
        };

        Ok(Some(domain.0))
    }

    /// Query for a domain's NameRecord, its parent's NameRecord if supplied, and the timestamp of
    /// the checkpoint bound.
    async fn query_domain_expiration(
        ctx: &Context<'_>,
        domain: &Domain,
        checkpoint_viewed_at: u64,
    ) -> Result<Option<DomainExpiration>, Error> {
        let config: &NameServiceConfig = ctx.data_unchecked();
        let db: &Db = ctx.data_unchecked();
        // Construct the list of `object_id`s to look up. The first element is the domain's
        // `NameRecord`. If the domain is a subdomain, there will be a second element for the
        // parent's `NameRecord`.
        let mut object_ids = vec![SuiAddress::from(config.record_field_id(&domain.0))];
        if domain.0.is_subdomain() {
            object_ids.push(SuiAddress::from(config.record_field_id(&domain.0.parent())));
        }

        // Create a page with a bound of `object_ids` length to fetch the relevant `NameRecord`s.
        let page: Page<object::Cursor> = Page::from_params(
            ctx.data_unchecked(),
            Some(object_ids.len() as u64),
            None,
            None,
            None,
        )
        .map_err(|_| {
            Error::Internal("Page size of 2 is incompatible with configured limits".to_string())
        })?;

        // prepare the filter for the query.
        let filter = ObjectFilter {
            object_ids: Some(object_ids.clone()),
            ..Default::default()
        };

        let Some((checkpoint_timestamp_ms, results)) = db
            .execute_repeatable(move |conn| {
                async move {
                    let Some(range) = AvailableRange::result(conn, checkpoint_viewed_at).await?
                    else {
                        return Ok::<_, diesel::result::Error>(None);
                    };

                    let timestamp_ms =
                        Checkpoint::query_timestamp(conn, checkpoint_viewed_at).await?;

                    let sql = build_objects_query(
                        View::Consistent,
                        range,
                        &page,
                        move |query| filter.apply(query),
                        move |newer| newer,
                    );

                    let objects: Vec<StoredHistoryObject> =
                        conn.results(move || sql.clone().into_boxed()).await?;

                    Ok(Some((timestamp_ms, objects)))
                }
                .scope_boxed()
            })
            .await?
        else {
            return Err(Error::Client(
                "Requested data is outside the available range".to_string(),
            ));
        };

        let mut domain_expiration = DomainExpiration {
            parent_name_record: None,
            name_record: None,
            checkpoint_timestamp_ms,
        };

        // Max size of results is 2. We loop through them, convert to objects, and then parse
        // name_record. We then assign it to the correct field on `domain_expiration` based on the
        // address.
        for result in results {
            let object =
                Object::try_from_stored_history_object(result, checkpoint_viewed_at, None)?;
            let move_object = MoveObject::try_from(&object).map_err(|_| {
                Error::Internal(format!(
                    "Expected {0} to be a NameRecord, but it's not a Move Object.",
                    object.address
                ))
            })?;

            let record = NameRecord::try_from(move_object.native)?;

            if object.address == object_ids[0] {
                domain_expiration.name_record = Some(record);
            } else if Some(&object.address) == object_ids.get(1) {
                domain_expiration.parent_name_record = Some(record);
            }
        }

        Ok(Some(domain_expiration))
    }
}

impl SuinsRegistration {
    /// Query the database for a `page` of SuiNS registrations. The page uses the same cursor type
    /// as is used for `Object`, and is further filtered to a particular `owner`. `config` specifies
    /// where to find the domain name registry and its type.
    ///
    /// `checkpoint_viewed_at` represents the checkpoint sequence number at which this page was
    /// queried for. Each entity returned in the connection will inherit this checkpoint, so that
    /// when viewing that entity's state, it will be as if it was read at the same checkpoint.
    pub(crate) async fn paginate(
        db: &Db,
        config: &NameServiceConfig,
        page: Page<object::Cursor>,
        owner: SuiAddress,
        checkpoint_viewed_at: u64,
    ) -> Result<Connection<String, SuinsRegistration>, Error> {
        let type_ = SuinsRegistration::type_(config.package_address.into());

        let filter = ObjectFilter {
            type_: Some(type_.clone().into()),
            owner: Some(owner),
            ..Default::default()
        };

        Object::paginate_subtype(db, page, filter, checkpoint_viewed_at, |object| {
            let address = object.address;
            let move_object = MoveObject::try_from(&object).map_err(|_| {
                Error::Internal(format!(
                    "Expected {address} to be a SuinsRegistration, but it's not a Move Object.",
                ))
            })?;

            SuinsRegistration::try_from(&move_object, &type_).map_err(|_| {
                Error::Internal(format!(
                    "Expected {address} to be a SuinsRegistration, but it is not."
                ))
            })
        })
        .await
    }

    /// Return the type representing a `SuinsRegistration` on chain. This can change from chain to
    /// chain (mainnet, testnet, devnet etc).
    pub(crate) fn type_(package: SuiAddress) -> StructTag {
        StructTag {
            address: package.into(),
            module: MOD_REGISTRATION.to_owned(),
            name: TYP_REGISTRATION.to_owned(),
            type_params: vec![],
        }
    }

    // Because the type of the SuinsRegistration object is not constant,
    // we need to take it in as a param.
    pub(crate) fn try_from(
        move_object: &MoveObject,
        tag: &StructTag,
    ) -> Result<Self, SuinsRegistrationDowncastError> {
        if !move_object.native.is_type(tag) {
            return Err(SuinsRegistrationDowncastError::NotASuinsRegistration);
        }

        Ok(Self {
            super_: move_object.clone(),
            native: bcs::from_bytes(move_object.native.contents())
                .map_err(SuinsRegistrationDowncastError::Bcs)?,
        })
    }
}

impl_string_input!(Domain);

impl FromStr for Domain {
    type Err = <NativeDomain as FromStr>::Err;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Domain(NativeDomain::from_str(s)?))
    }
}
