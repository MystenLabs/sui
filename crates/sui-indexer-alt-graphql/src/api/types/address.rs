// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{
    connection::{Connection, Edge},
    Context, Interface, Object,
};
use futures::future::try_join_all;
use sui_types::{base_types::SuiAddress as NativeSuiAddress, dynamic_field::DynamicFieldType};

use crate::{
    api::scalars::{owner_kind::OwnerKind, sui_address::SuiAddress, type_filter::TypeInput},
    error::RpcError,
    pagination::{Page, PaginationConfig},
    scope::Scope,
};

use super::{
    balance::{self, Balance},
    dynamic_field::{DynamicField, DynamicFieldName},
    move_object::MoveObject,
    move_package::MovePackage,
    object::{self, Object, ObjectKey},
    object_filter::{ObjectFilter, Validator as OFValidator},
};

/// Interface implemented by GraphQL types representing entities that are identified by an address.
///
/// An address uniquely represents either the public key of an account, or an object's ID, but never both. It is not possible to determine which type an address represents up-front. If an object is wrapped, its contents will not be accessible via its address, but it will still be possible to access other objects it owns.
#[allow(clippy::duplicated_attributes)]
#[derive(Interface)]
#[graphql(
    name = "IAddressable",
    field(name = "address", ty = "SuiAddress"),
    field(
        name = "balance",
        arg(name = "coin_type", ty = "TypeInput"),
        ty = "Result<Option<Balance>, RpcError<balance::Error>>",
        desc = "Fetch the total balance for coins with marker type `coinType` (e.g. `0x2::sui::SUI`), owned by this address.\n\nIf the address does not own any coins of that type, a balance of zero is returned.",
    ),
    field(
        name = "balances",
        arg(name = "first", ty = "Option<u64>"),
        arg(name = "after", ty = "Option<balance::Cursor>"),
        arg(name = "last", ty = "Option<u64>"),
        arg(name = "before", ty = "Option<balance::Cursor>"),
        ty = "Result<Option<Connection<String, Balance>>, RpcError<balance::Error>>",
        desc = "Total balance across coins owned by this address, grouped by coin type.",
    ),
    field(
        name = "multi_get_balances",
        arg(name = "keys", ty = "Vec<TypeInput>"),
        ty = "Result<Option<Vec<Balance>>, RpcError<balance::Error>>",
        desc = "Fetch the total balances keyed by coin types (e.g. `0x2::sui::SUI`) owned by this address.\n\nReturns `null` when no checkpoint is set in scope (e.g. execution scope). If the address does not own any coins of a given type, a balance of zero is returned for that type.",
    ),
    field(
        name = "objects",
        arg(name = "first", ty = "Option<u64>"),
        arg(name = "after", ty = "Option<object::CLive>"),
        arg(name = "last", ty = "Option<u64>"),
        arg(name = "before", ty = "Option<object::CLive>"),
        arg(name = "filter", ty = "Option<ObjectFilter>"),
        ty = "Result<Option<Connection<String, MoveObject>>, RpcError<object::Error>>",
        desc = "Objects owned by this address, optionally filtered by type."
    )
)]
pub(crate) enum IAddressable {
    Address(Address),
    DynamicField(DynamicField),
    MoveObject(MoveObject),
    MovePackage(MovePackage),
    Object(Object),
}

#[derive(Clone)]
pub(crate) struct Address {
    pub(crate) scope: Scope,
    pub(crate) address: NativeSuiAddress,
}

#[Object]
impl Address {
    /// The Address' identifier, a 32-byte number represented as a 64-character hex string, with a lead "0x".
    pub(crate) async fn address(&self) -> Result<SuiAddress, RpcError> {
        Ok(self.address.into())
    }

    /// Attempts to fetch the object at this address.
    pub(crate) async fn as_object(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<Object>, RpcError<object::Error>> {
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
    }

    /// Fetch the total balance for coins with marker type `coinType` (e.g. `0x2::sui::SUI`), owned by this address.
    ///
    /// Returns `None` when no checkpoint is set in scope (e.g. execution scope).
    /// If the address does not own any coins of that type, a balance of zero is returned.
    pub(crate) async fn balance(
        &self,
        ctx: &Context<'_>,
        coin_type: TypeInput,
    ) -> Result<Option<Balance>, RpcError<balance::Error>> {
        Balance::fetch_one(ctx, &self.scope, self.address, coin_type.into()).await
    }

    /// Total balance across coins owned by this address, grouped by coin type.
    pub(crate) async fn balances(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<balance::Cursor>,
        last: Option<u64>,
        before: Option<balance::Cursor>,
    ) -> Result<Option<Connection<String, Balance>>, RpcError<balance::Error>> {
        let pagination: &PaginationConfig = ctx.data()?;
        let limits = pagination.limits("IAddressable", "balances");
        let page = Page::from_params(limits, first, after, last, before)?;
        Balance::paginate(ctx, self.scope.clone(), self.address, page)
            .await
            .map(Some)
    }

    /// Access a dynamic field on an object using its type and BCS-encoded name.
    pub(crate) async fn dynamic_field(
        &self,
        ctx: &Context<'_>,
        name: DynamicFieldName,
    ) -> Result<Option<DynamicField>, RpcError<object::Error>> {
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
    pub(crate) async fn dynamic_object_field(
        &self,
        ctx: &Context<'_>,
        name: DynamicFieldName,
    ) -> Result<Option<DynamicField>, RpcError<object::Error>> {
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
    ) -> Result<Vec<Option<DynamicField>>, RpcError<object::Error>> {
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
    ) -> Result<Vec<Option<DynamicField>>, RpcError<object::Error>> {
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
    ) -> Result<Option<Vec<Balance>>, RpcError<balance::Error>> {
        let coin_types = keys.into_iter().map(|k| k.into()).collect();
        Balance::fetch_many(ctx, &self.scope, self.address, coin_types).await
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
    ) -> Result<Option<Connection<String, MoveObject>>, RpcError<object::Error>> {
        let pagination: &PaginationConfig = ctx.data()?;
        let limits = pagination.limits("IAddressable", "objects");
        let page = Page::from_params(limits, first, after, last, before)?;

        // Create a filter that constrains to ADDRESS kind and this owner
        let Some(filter) = filter.unwrap_or_default().intersect(ObjectFilter {
            owner_kind: Some(OwnerKind::Address),
            owner: Some(self.address.into()),
            ..Default::default()
        }) else {
            return Ok(Some(Connection::new(false, false)));
        };

        let objects = Object::paginate_live(ctx, self.scope.clone(), page, filter).await?;
        let mut move_objects = Connection::new(objects.has_previous_page, objects.has_next_page);

        for edge in objects.edges {
            let move_obj = MoveObject::from_super(edge.node);
            move_objects.edges.push(Edge::new(edge.cursor, move_obj));
        }

        Ok(Some(move_objects))
    }
}

impl Address {
    /// Construct an address that is represented by just its identifier (`SuiAddress`).
    /// This does not check whether the address is valid or exists in the system.
    pub(crate) fn with_address(scope: Scope, address: NativeSuiAddress) -> Self {
        Self { scope, address }
    }
}
