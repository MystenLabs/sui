// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use async_graphql::Context;
use async_graphql::Enum;
use async_graphql::InputObject;
use async_graphql::Interface;
use async_graphql::Object;
use async_graphql::connection::Connection;
use async_graphql::connection::Edge;
use futures::future::try_join_all;
use sui_types::base_types::SuiAddress as NativeSuiAddress;
use sui_types::dynamic_field::DynamicFieldType;

use crate::api::scalars::id::Id;
use crate::api::scalars::owner_kind::OwnerKind;
use crate::api::scalars::sui_address::SuiAddress;
use crate::api::scalars::type_filter::TypeInput;
use crate::api::scalars::uint53::UInt53;
use crate::api::types::balance::Balance;
use crate::api::types::balance::{self as balance};
use crate::api::types::coin_metadata::CoinMetadata;
use crate::api::types::dynamic_field;
use crate::api::types::dynamic_field::DynamicField;
use crate::api::types::dynamic_field::DynamicFieldName;
use crate::api::types::move_object::MoveObject;
use crate::api::types::move_package::MovePackage;
use crate::api::types::name_service::address_to_name;
use crate::api::types::object::Object;
use crate::api::types::object::ObjectKey;
use crate::api::types::object::{self as object};
use crate::api::types::object_filter::ObjectFilter;
use crate::api::types::object_filter::ObjectFilterValidator as OFValidator;
use crate::api::types::transaction::CTransaction;
use crate::api::types::transaction::Transaction;
use crate::api::types::transaction::filter::TransactionFilter;
use crate::api::types::transaction::filter::TransactionFilterValidator as TFValidator;
use crate::error::RpcError;
use crate::error::bad_user_input;
use crate::pagination::Page;
use crate::pagination::PaginationConfig;
use crate::scope::Scope;
use crate::task::watermark::Watermarks;

/// The possible relationship types for a transaction: sent or affected.
#[derive(Enum, Copy, Clone, Eq, PartialEq)]
pub(crate) enum AddressTransactionRelationship {
    /// Transactions this address has sent.
    Sent,
    /// Transactions that this address was involved in, either as the sender, sponsor, or as the owner of some object that was created, modified or transferred.
    Affected,
}

/// Interface implemented by GraphQL types representing entities that are identified by an address.
///
/// An address uniquely represents either the public key of an account, or an object's ID, but never both. It is not possible to determine which type an address represents up-front. If an object is wrapped, its contents will not be accessible via its address, but it will still be possible to access other objects it owns.
#[allow(clippy::duplicated_attributes)]
#[derive(Interface)]
#[graphql(
    name = "IAddressable",
    field(name = "address", ty = "SuiAddress"),
    field(
        name = "address_at",
        arg(name = "root_version", ty = "Option<UInt53>"),
        arg(name = "checkpoint", ty = "Option<UInt53>"),
        ty = "Result<Option<Address>, RpcError<Error>>",
        desc = "Fetch the address as it was at a different root version, or checkpoint.\n\nIf no additional bound is provided, the address is fetched at the latest checkpoint known to the RPC.",
    ),
    field(
        name = "balance",
        arg(name = "coin_type", ty = "TypeInput"),
        ty = "Option<Result<Balance, RpcError<balance::Error>>>",
        desc = "Fetch the total balance for coins with marker type `coinType` (e.g. `0x2::sui::SUI`), owned by this address.\n\nIf the address does not own any coins of that type, a balance of zero is returned.",
    ),
    field(
        name = "balances",
        arg(name = "first", ty = "Option<u64>"),
        arg(name = "after", ty = "Option<balance::Cursor>"),
        arg(name = "last", ty = "Option<u64>"),
        arg(name = "before", ty = "Option<balance::Cursor>"),
        ty = "Option<Result<Connection<String, Balance>, RpcError<balance::Error>>>",
        desc = "Total balance across coins owned by this address, grouped by coin type.",
    ),
    field(
        name = "default_suins_name",
        ty = "Option<Result<String, RpcError>>",
        desc = "The domain explicitly configured as the default SuiNS name for this address."
    ),
    field(
        name = "multi_get_balances",
        arg(name = "keys", ty = "Vec<TypeInput>"),
        ty = "Option<Result<Vec<Balance>, RpcError<balance::Error>>>",
        desc = "Fetch the total balances keyed by coin types (e.g. `0x2::sui::SUI`) owned by this address.\n\nReturns `null` when no checkpoint is set in scope (e.g. execution scope). If the address does not own any coins of a given type, a balance of zero is returned for that type.",
    ),
    field(
        name = "objects",
        arg(name = "first", ty = "Option<u64>"),
        arg(name = "after", ty = "Option<object::CLive>"),
        arg(name = "last", ty = "Option<u64>"),
        arg(name = "before", ty = "Option<object::CLive>"),
        arg(name = "filter", ty = "Option<ObjectFilter>"),
        ty = "Option<Result<Connection<String, MoveObject>, RpcError<object::Error>>>",
        desc = "Objects owned by this address, optionally filtered by type."
    )
)]
pub(crate) enum IAddressable {
    Address(Address),
    CoinMetadata(CoinMetadata),
    DynamicField(DynamicField),
    MoveObject(MoveObject),
    MovePackage(MovePackage),
    Object(Object),
}

#[derive(Clone, Debug)]
pub(crate) struct Address {
    pub(crate) scope: Scope,
    pub(crate) address: NativeSuiAddress,
}

/// Identifies a specific version of an address.
///
/// The `address` field must be specified, as well as at most one of `rootVersion`, or `atCheckpoint`. If neither is provided, the package is fetched at the checkpoint being viewed.
///
/// See `Query.address` for more details.
#[derive(InputObject, Debug, Clone, Eq, PartialEq)]
pub(crate) struct AddressKey {
    /// The address.
    pub(crate) address: SuiAddress,

    /// If specified, sets a root version bound for this address.
    pub(crate) root_version: Option<UInt53>,

    /// If specified, sets a checkpoint bound for this address.
    pub(crate) at_checkpoint: Option<UInt53>,
}

#[derive(thiserror::Error, Debug, Clone)]
pub(crate) enum Error {
    #[error("Checkpoint {0} in the future")]
    Future(u64),

    #[error(
        "At most one of a root version, or a checkpoint bound can be specified when fetching an address"
    )]
    OneBound,
}

#[Object]
impl Address {
    /// The address's globally unique identifier, which can be passed to `Query.node` to refetch it.
    pub(crate) async fn id(&self) -> Id {
        Id::Address(self.address)
    }

    /// The Address' identifier, a 32-byte number represented as a 64-character hex string, with a lead "0x".
    pub(crate) async fn address(&self) -> Result<SuiAddress, RpcError> {
        Ok(self.address.into())
    }

    /// Fetch the address as it was at a different root version, or checkpoint.
    ///
    /// If no additional bound is provided, the address is fetched at the latest checkpoint known to the RPC.
    pub(crate) async fn address_at(
        &self,
        ctx: &Context<'_>,
        root_version: Option<UInt53>,
        checkpoint: Option<UInt53>,
    ) -> Result<Option<Address>, RpcError<Error>> {
        Ok(Some(Address::by_key(
            ctx,
            Scope::new(ctx)?,
            AddressKey {
                address: self.address.into(),
                root_version,
                at_checkpoint: checkpoint,
            },
        )?))
    }

    /// Attempts to fetch the object at this address.
    pub(crate) async fn as_object(
        &self,
        ctx: &Context<'_>,
    ) -> Option<Result<Object, RpcError<object::Error>>> {
        Object::by_key(
            ctx,
            self.scope.clone(),
            ObjectKey {
                address: self.address.into(),
                version: None,
                root_version: self.scope.root_version().map(Into::into),
                at_checkpoint: None,
            },
        )
        .await
        .transpose()
    }

    /// Fetch the total balance for coins with marker type `coinType` (e.g. `0x2::sui::SUI`), owned by this address.
    ///
    /// Returns `None` when no checkpoint is set in scope (e.g. execution scope).
    /// If the address does not own any coins of that type, a balance of zero is returned.
    pub(crate) async fn balance(
        &self,
        ctx: &Context<'_>,
        coin_type: TypeInput,
    ) -> Option<Result<Balance, RpcError<balance::Error>>> {
        Balance::fetch_one(ctx, &self.scope, self.address, coin_type.into())
            .await
            .transpose()
    }

    /// Total balance across coins owned by this address, grouped by coin type.
    pub(crate) async fn balances(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<balance::Cursor>,
        last: Option<u64>,
        before: Option<balance::Cursor>,
    ) -> Option<Result<Connection<String, Balance>, RpcError<balance::Error>>> {
        Some(
            async {
                let pagination: &PaginationConfig = ctx.data()?;
                let limits = pagination.limits("IAddressable", "balances");
                let page = Page::from_params(limits, first, after, last, before)?;
                Balance::paginate(ctx, self.scope.clone(), self.address, page).await
            }
            .await,
        )
    }

    /// The domain explicitly configured as the default SuiNS name for this address.
    pub(crate) async fn default_suins_name(
        &self,
        ctx: &Context<'_>,
    ) -> Option<Result<String, RpcError>> {
        address_to_name(ctx, &self.scope, self.address)
            .await
            .transpose()
    }

    /// Access a dynamic field on an object using its type and BCS-encoded name.
    ///
    /// Returns `null` if a dynamic field with that name could not be found attached to the object with this address.
    pub(crate) async fn dynamic_field(
        &self,
        ctx: &Context<'_>,
        name: DynamicFieldName,
    ) -> Result<Option<DynamicField>, RpcError<dynamic_field::Error>> {
        DynamicField::by_name(
            ctx,
            self.scope.clone(),
            self.address.into(),
            DynamicFieldType::DynamicField,
            name,
        )
        .await
    }

    /// Dynamic fields owned by this address.
    ///
    /// The address must correspond to an object (account addresses cannot own dynamic fields), but that object may be wrapped.
    pub(crate) async fn dynamic_fields(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<object::CLive>,
        last: Option<u64>,
        before: Option<object::CLive>,
    ) -> Result<Option<Connection<String, DynamicField>>, RpcError<object::Error>> {
        let pagination: &PaginationConfig = ctx.data()?;
        let limits = pagination.limits("Address", "dynamicFields");
        let page = Page::from_params(limits, first, after, last, before)?;

        let dynamic_fields =
            DynamicField::paginate(ctx, self.scope.clone(), self.address.into(), page).await?;

        Ok(Some(dynamic_fields))
    }

    /// Access a dynamic object field on an object using its type and BCS-encoded name.
    ///
    /// Returns `null` if a dynamic object field with that name could not be found attached to the object with this address.
    pub(crate) async fn dynamic_object_field(
        &self,
        ctx: &Context<'_>,
        name: DynamicFieldName,
    ) -> Result<Option<DynamicField>, RpcError<dynamic_field::Error>> {
        DynamicField::by_name(
            ctx,
            self.scope.clone(),
            self.address.into(),
            DynamicFieldType::DynamicObject,
            name,
        )
        .await
    }

    /// Access dynamic fields on an object using their types and BCS-encoded names.
    ///
    /// Returns a list of dynamic fields that is guaranteed to be the same length as `keys`. If a dynamic field in `keys` could not be found in the store, its corresponding entry in the result will be `null`.
    pub(crate) async fn multi_get_dynamic_fields(
        &self,
        ctx: &Context<'_>,
        keys: Vec<DynamicFieldName>,
    ) -> Result<Vec<Option<DynamicField>>, RpcError<dynamic_field::Error>> {
        try_join_all(keys.into_iter().map(|key| {
            DynamicField::by_name(
                ctx,
                self.scope.clone(),
                self.address.into(),
                DynamicFieldType::DynamicField,
                key,
            )
        }))
        .await
    }

    /// Access dynamic object fields on an object using their types and BCS-encoded names.
    ///
    /// Returns a list of dynamic object fields that is guaranteed to be the same length as `keys`. If a dynamic object field in `keys` could not be found in the store, its corresponding entry in the result will be `null`.
    pub(crate) async fn multi_get_dynamic_object_fields(
        &self,
        ctx: &Context<'_>,
        keys: Vec<DynamicFieldName>,
    ) -> Result<Vec<Option<DynamicField>>, RpcError<dynamic_field::Error>> {
        try_join_all(keys.into_iter().map(|key| {
            DynamicField::by_name(
                ctx,
                self.scope.clone(),
                self.address.into(),
                DynamicFieldType::DynamicObject,
                key,
            )
        }))
        .await
    }

    /// Fetch the total balances keyed by coin types (e.g. `0x2::sui::SUI`) owned by this address.
    ///
    /// Returns `None` when no checkpoint is set in scope (e.g. execution scope).
    /// If the address does not own any coins of a given type, a balance of zero is returned for that type.
    pub(crate) async fn multi_get_balances(
        &self,
        ctx: &Context<'_>,
        keys: Vec<TypeInput>,
    ) -> Option<Result<Vec<Balance>, RpcError<balance::Error>>> {
        let coin_types = keys.into_iter().map(|k| k.into()).collect();
        Balance::fetch_many(ctx, &self.scope, self.address, coin_types)
            .await
            .transpose()
    }

    /// Objects owned by this address, optionally filtered by type.
    pub(crate) async fn objects(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<object::CLive>,
        last: Option<u64>,
        before: Option<object::CLive>,
        #[graphql(validator(custom = "OFValidator::allows_empty()"))] filter: Option<ObjectFilter>,
    ) -> Option<Result<Connection<String, MoveObject>, RpcError<object::Error>>> {
        Some(
            async {
                let pagination: &PaginationConfig = ctx.data()?;
                let limits = pagination.limits("IAddressable", "objects");
                let page = Page::from_params(limits, first, after, last, before)?;

                // Create a filter that constrains to ADDRESS kind and this owner
                let Some(filter) = filter.unwrap_or_default().intersect(ObjectFilter {
                    owner_kind: Some(OwnerKind::Address),
                    owner: Some(self.address.into()),
                    ..Default::default()
                }) else {
                    return Ok(Connection::new(false, false));
                };

                let objects = Object::paginate_live(ctx, self.scope.clone(), page, filter).await?;
                let mut move_objects =
                    Connection::new(objects.has_previous_page, objects.has_next_page);

                for edge in objects.edges {
                    let move_obj = MoveObject::from_super(edge.node);
                    move_objects.edges.push(Edge::new(edge.cursor, move_obj));
                }

                Ok(move_objects)
            }
            .await,
        )
    }

    /// Transactions associated with this address.
    ///
    /// Similar behavior to the `transactions` in Query but supporting the additional `AddressTransactionRelationship` filter, which defaults to `SENT`.
    pub(crate) async fn transactions(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CTransaction>,
        last: Option<u64>,
        before: Option<CTransaction>,
        relation: Option<AddressTransactionRelationship>,
        #[graphql(validator(custom = "TFValidator"))] filter: Option<TransactionFilter>,
    ) -> Option<Result<Connection<String, Transaction>, RpcError>> {
        Some(
            async {
                let pagination: &PaginationConfig = ctx.data()?;
                let limits = pagination.limits("Address", "transactions");
                let page = Page::from_params(limits, first, after, last, before)?;

                // Default relation to SENT if not provided
                let relation = relation.unwrap_or(AddressTransactionRelationship::Sent);

                // Create address-specific filter based on relationship
                let address_filter = match relation {
                    AddressTransactionRelationship::Sent => TransactionFilter {
                        sent_address: Some(self.address.into()),
                        ..Default::default()
                    },
                    AddressTransactionRelationship::Affected => TransactionFilter {
                        affected_address: Some(self.address.into()),
                        ..Default::default()
                    },
                };

                // Intersect with user-provided filter
                let Some(filter) = filter.unwrap_or_default().intersect(address_filter) else {
                    return Ok(Connection::new(false, false));
                };

                Transaction::paginate(ctx, self.scope.clone(), page, filter).await
            }
            .await,
        )
    }
}

impl Address {
    /// Fetch an address by its key. The key can either specify a root version bound, or a
    /// checkpoint bound, or neither.
    pub(crate) fn by_key(
        ctx: &Context<'_>,
        scope: Scope,
        key: AddressKey,
    ) -> Result<Self, RpcError<Error>> {
        let bounds = key.root_version.is_some() as u8 + key.at_checkpoint.is_some() as u8;

        if bounds > 1 {
            Err(bad_user_input(Error::OneBound))
        } else if let Some(v) = key.root_version {
            let scope = scope.with_root_version(v.into());
            Ok(Self::with_address(scope, key.address.into()))
        } else if let Some(cp) = key.at_checkpoint {
            // Validate checkpoint isn't in the future
            let watermark: &Arc<Watermarks> = ctx.data()?;
            if u64::from(cp) > watermark.high_watermark().checkpoint() {
                return Err(bad_user_input(Error::Future(cp.into())));
            }

            let scope = scope.with_root_checkpoint(cp.into());
            Ok(Self::with_address(scope, key.address.into()))
        } else {
            Ok(Self::with_address(scope, key.address.into()))
        }
    }

    /// Construct an address that is represented by just its identifier (`SuiAddress`).
    /// This does not check whether the address is valid or exists in the system.
    pub(crate) fn with_address(scope: Scope, address: NativeSuiAddress) -> Self {
        Self { scope, address }
    }
}
