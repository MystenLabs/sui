// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{
    connection::{Connection, Edge},
    Context, Interface, Object,
};
use sui_types::base_types::SuiAddress as NativeSuiAddress;

use crate::{
    api::scalars::{owner_kind::OwnerKind, sui_address::SuiAddress},
    error::RpcError,
    pagination::{Page, PaginationConfig},
    scope::Scope,
};

use super::{
    move_object::MoveObject,
    move_package::MovePackage,
    object::{self, Object},
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
    MoveObject(MoveObject),
    MovePackage(MovePackage),
    Object(Object),
}

#[derive(Clone)]
pub(crate) struct Address {
    pub(crate) scope: Scope,
    pub(crate) address: NativeSuiAddress,
}

pub(crate) struct AddressableImpl<'a>(&'a Address);

#[Object]
impl Address {
    /// The Address' identifier, a 32-byte number represented as a 64-character hex string, with a lead "0x".
    pub(crate) async fn address(&self) -> SuiAddress {
        AddressableImpl::from(self).address()
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
        AddressableImpl::from(self)
            .objects(ctx, first, after, last, before, filter)
            .await
    }
}

impl Address {
    /// Construct an address that is represented by just its identifier (`SuiAddress`).
    /// This does not check whether the address is valid or exists in the system.
    pub(crate) fn with_address(scope: Scope, address: NativeSuiAddress) -> Self {
        Self { scope, address }
    }
}

impl AddressableImpl<'_> {
    pub(crate) fn address(&self) -> SuiAddress {
        self.0.address.into()
    }

    pub(crate) async fn objects(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<object::CLive>,
        last: Option<u64>,
        before: Option<object::CLive>,
        filter: Option<ObjectFilter>,
    ) -> Result<Option<Connection<String, MoveObject>>, RpcError<object::Error>> {
        let pagination: &PaginationConfig = ctx.data()?;
        let limits = pagination.limits("IAddressable", "objects");
        let page = Page::from_params(limits, first, after, last, before)?;

        // Create a filter that constrains to ADDRESS kind and this owner
        let Some(filter) = filter.unwrap_or_default().intersect(ObjectFilter {
            owner_kind: Some(OwnerKind::Address),
            owner: Some(self.address()),
            ..Default::default()
        }) else {
            return Ok(Some(Connection::new(false, false)));
        };

        let objects = Object::paginate_live(ctx, self.0.scope.clone(), page, filter).await?;
        let mut move_objects = Connection::new(objects.has_previous_page, objects.has_next_page);

        for edge in objects.edges {
            let move_obj = MoveObject::from_super(edge.node);
            move_objects.edges.push(Edge::new(edge.cursor, move_obj));
        }

        Ok(Some(move_objects))
    }
}

impl<'a> From<&'a Address> for AddressableImpl<'a> {
    fn from(address: &'a Address) -> Self {
        AddressableImpl(address)
    }
}
