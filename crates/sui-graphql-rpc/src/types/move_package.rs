// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::balance::{self, Balance};
use super::base64::Base64;
use super::big_int::BigInt;
use super::coin::Coin;
use super::cursor::{JsonCursor, Page};
use super::move_module::MoveModule;
use super::move_object::MoveObject;
use super::object::{
    self, Object, ObjectFilter, ObjectImpl, ObjectLookup, ObjectOwner, ObjectStatus,
};
use super::owner::OwnerImpl;
use super::stake::StakedSui;
use super::sui_address::SuiAddress;
use super::suins_registration::{DomainFormat, SuinsRegistration};
use super::transaction_block::{self, TransactionBlock, TransactionBlockFilter};
use super::type_filter::ExactTypeFilter;
use super::uint53::UInt53;
use crate::consistency::ConsistentNamedCursor;
use crate::error::Error;
use async_graphql::connection::{Connection, CursorType, Edge};
use async_graphql::*;
use sui_package_resolver::{error::Error as PackageCacheError, Package as ParsedMovePackage};
use sui_types::{move_package::MovePackage as NativeMovePackage, object::Data};

#[derive(Clone)]
pub(crate) struct MovePackage {
    /// Representation of this Move Object as a generic Object.
    pub super_: Object,

    /// Move-object-specific data, extracted from the native representation at
    /// `graphql_object.native_object.data`.
    pub native: NativeMovePackage,
}

/// Information used by a package to link to a specific version of its dependency.
#[derive(SimpleObject)]
struct Linkage {
    /// The ID on-chain of the first version of the dependency.
    original_id: SuiAddress,

    /// The ID on-chain of the version of the dependency that this package depends on.
    upgraded_id: SuiAddress,

    /// The version of the dependency that this package depends on.
    version: UInt53,
}

/// Information about which previous versions of a package introduced its types.
#[derive(SimpleObject)]
struct TypeOrigin {
    /// Module defining the type.
    module: String,

    /// Name of the struct.
    #[graphql(name = "struct")]
    struct_: String,

    /// The storage ID of the package that first defined this type.
    defining_id: SuiAddress,
}

pub(crate) struct MovePackageDowncastError;

pub(crate) type CModule = JsonCursor<ConsistentNamedCursor>;

/// A MovePackage is a kind of Move object that represents code that has been published on chain.
/// It exposes information about its modules, type definitions, functions, and dependencies.
#[Object]
impl MovePackage {
    pub(crate) async fn address(&self) -> SuiAddress {
        OwnerImpl::from(&self.super_).address().await
    }

    /// Objects owned by this package, optionally `filter`-ed.
    ///
    /// Note that objects owned by a package are inaccessible, because packages are immutable and
    /// cannot be owned by an address.
    pub(crate) async fn objects(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<object::Cursor>,
        last: Option<u64>,
        before: Option<object::Cursor>,
        filter: Option<ObjectFilter>,
    ) -> Result<Connection<String, MoveObject>> {
        OwnerImpl::from(&self.super_)
            .objects(ctx, first, after, last, before, filter)
            .await
    }

    /// Total balance of all coins with marker type owned by this package. If type is not supplied,
    /// it defaults to `0x2::sui::SUI`.
    ///
    /// Note that coins owned by a package are inaccessible, because packages are immutable and
    /// cannot be owned by an address.
    pub(crate) async fn balance(
        &self,
        ctx: &Context<'_>,
        type_: Option<ExactTypeFilter>,
    ) -> Result<Option<Balance>> {
        OwnerImpl::from(&self.super_).balance(ctx, type_).await
    }

    /// The balances of all coin types owned by this package.
    ///
    /// Note that coins owned by a package are inaccessible, because packages are immutable and
    /// cannot be owned by an address.
    pub(crate) async fn balances(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<balance::Cursor>,
        last: Option<u64>,
        before: Option<balance::Cursor>,
    ) -> Result<Connection<String, Balance>> {
        OwnerImpl::from(&self.super_)
            .balances(ctx, first, after, last, before)
            .await
    }

    /// The coin objects owned by this package.
    ///
    ///`type` is a filter on the coin's type parameter, defaulting to `0x2::sui::SUI`.
    ///
    /// Note that coins owned by a package are inaccessible, because packages are immutable and
    /// cannot be owned by an address.
    pub(crate) async fn coins(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<object::Cursor>,
        last: Option<u64>,
        before: Option<object::Cursor>,
        type_: Option<ExactTypeFilter>,
    ) -> Result<Connection<String, Coin>> {
        OwnerImpl::from(&self.super_)
            .coins(ctx, first, after, last, before, type_)
            .await
    }

    /// The `0x3::staking_pool::StakedSui` objects owned by this package.
    ///
    /// Note that objects owned by a package are inaccessible, because packages are immutable and
    /// cannot be owned by an address.
    pub(crate) async fn staked_suis(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<object::Cursor>,
        last: Option<u64>,
        before: Option<object::Cursor>,
    ) -> Result<Connection<String, StakedSui>> {
        OwnerImpl::from(&self.super_)
            .staked_suis(ctx, first, after, last, before)
            .await
    }

    /// The domain explicitly configured as the default domain pointing to this object.
    pub(crate) async fn default_suins_name(
        &self,
        ctx: &Context<'_>,
        format: Option<DomainFormat>,
    ) -> Result<Option<String>> {
        OwnerImpl::from(&self.super_)
            .default_suins_name(ctx, format)
            .await
    }

    /// The SuinsRegistration NFTs owned by this package. These grant the owner the capability to
    /// manage the associated domain.
    ///
    /// Note that objects owned by a package are inaccessible, because packages are immutable and
    /// cannot be owned by an address.
    pub(crate) async fn suins_registrations(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<object::Cursor>,
        last: Option<u64>,
        before: Option<object::Cursor>,
    ) -> Result<Connection<String, SuinsRegistration>> {
        OwnerImpl::from(&self.super_)
            .suins_registrations(ctx, first, after, last, before)
            .await
    }

    pub(crate) async fn version(&self) -> UInt53 {
        ObjectImpl(&self.super_).version().await
    }

    /// The current status of the object as read from the off-chain store. The possible states are:
    /// NOT_INDEXED, the object is loaded from serialized data, such as the contents of a genesis or
    /// system package upgrade transaction. LIVE, the version returned is the most recent for the
    /// object, and it is not deleted or wrapped at that version. HISTORICAL, the object was
    /// referenced at a specific version or checkpoint, so is fetched from historical tables and may
    /// not be the latest version of the object. WRAPPED_OR_DELETED, the object is deleted or
    /// wrapped and only partial information can be loaded."
    pub(crate) async fn status(&self) -> ObjectStatus {
        ObjectImpl(&self.super_).status().await
    }

    /// 32-byte hash that identifies the package's contents, encoded as a Base58 string.
    pub(crate) async fn digest(&self) -> Option<String> {
        ObjectImpl(&self.super_).digest().await
    }

    /// The owner type of this object: Immutable, Shared, Parent, Address
    /// Packages are always Immutable.
    pub(crate) async fn owner(&self, ctx: &Context<'_>) -> Option<ObjectOwner> {
        ObjectImpl(&self.super_).owner(ctx).await
    }

    /// The transaction block that published or upgraded this package.
    pub(crate) async fn previous_transaction_block(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<TransactionBlock>> {
        ObjectImpl(&self.super_)
            .previous_transaction_block(ctx)
            .await
    }

    /// The amount of SUI we would rebate if this object gets deleted or mutated. This number is
    /// recalculated based on the present storage gas price.
    ///
    /// Note that packages cannot be deleted or mutated, so this number is provided purely for
    /// reference.
    pub(crate) async fn storage_rebate(&self) -> Option<BigInt> {
        ObjectImpl(&self.super_).storage_rebate().await
    }

    /// The transaction blocks that sent objects to this package.
    ///
    /// Note that objects that have been sent to a package become inaccessible.
    pub(crate) async fn received_transaction_blocks(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<transaction_block::Cursor>,
        last: Option<u64>,
        before: Option<transaction_block::Cursor>,
        filter: Option<TransactionBlockFilter>,
    ) -> Result<Connection<String, TransactionBlock>> {
        ObjectImpl(&self.super_)
            .received_transaction_blocks(ctx, first, after, last, before, filter)
            .await
    }

    /// The Base64-encoded BCS serialization of the package's content.
    pub(crate) async fn bcs(&self) -> Result<Option<Base64>> {
        ObjectImpl(&self.super_).bcs().await
    }

    /// A representation of the module called `name` in this package, including the
    /// structs and functions it defines.
    async fn module(&self, name: String) -> Result<Option<MoveModule>> {
        self.module_impl(&name).extend()
    }

    /// Paginate through the MoveModules defined in this package.
    pub async fn modules(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CModule>,
        last: Option<u64>,
        before: Option<CModule>,
    ) -> Result<Option<Connection<String, MoveModule>>> {
        use std::ops::Bound as B;

        let page = Page::from_params(ctx.data_unchecked(), first, after, last, before)?;
        let cursor_viewed_at = page.validate_cursor_consistency()?;
        let checkpoint_viewed_at =
            cursor_viewed_at.unwrap_or_else(|| self.checkpoint_viewed_at_impl());

        let parsed = self.parsed_package()?;
        let module_range = parsed.modules().range::<String, _>((
            page.after().map_or(B::Unbounded, |a| B::Excluded(&a.name)),
            page.before().map_or(B::Unbounded, |b| B::Excluded(&b.name)),
        ));

        let mut connection = Connection::new(false, false);
        let modules = if page.is_from_front() {
            module_range.take(page.limit()).collect()
        } else {
            let mut ms: Vec<_> = module_range.rev().take(page.limit()).collect();
            ms.reverse();
            ms
        };

        connection.has_previous_page = modules.first().is_some_and(|(fst, _)| {
            parsed
                .modules()
                .range::<String, _>((B::Unbounded, B::Excluded(*fst)))
                .next()
                .is_some()
        });

        connection.has_next_page = modules.last().is_some_and(|(lst, _)| {
            parsed
                .modules()
                .range::<String, _>((B::Excluded(*lst), B::Unbounded))
                .next()
                .is_some()
        });

        for (name, parsed) in modules {
            let Some(native) = self.native.serialized_module_map().get(name) else {
                return Err(Error::Internal(format!(
                    "Module '{name}' exists in PackageCache but not in serialized map.",
                ))
                .extend());
            };

            let cursor = JsonCursor::new(ConsistentNamedCursor {
                name: name.clone(),
                c: checkpoint_viewed_at,
            })
            .encode_cursor();
            connection.edges.push(Edge::new(
                cursor,
                MoveModule {
                    storage_id: self.super_.address,
                    native: native.clone(),
                    parsed: parsed.clone(),
                    checkpoint_viewed_at,
                },
            ))
        }

        if connection.edges.is_empty() {
            Ok(None)
        } else {
            Ok(Some(connection))
        }
    }

    /// The transitive dependencies of this package.
    async fn linkage(&self) -> Option<Vec<Linkage>> {
        let linkage = self
            .native
            .linkage_table()
            .iter()
            .map(|(&runtime_id, upgrade_info)| Linkage {
                original_id: runtime_id.into(),
                upgraded_id: upgrade_info.upgraded_id.into(),
                version: upgrade_info.upgraded_version.value().into(),
            })
            .collect();

        Some(linkage)
    }

    /// The (previous) versions of this package that introduced its types.
    async fn type_origins(&self) -> Option<Vec<TypeOrigin>> {
        let type_origins = self
            .native
            .type_origin_table()
            .iter()
            .map(|origin| TypeOrigin {
                module: origin.module_name.clone(),
                struct_: origin.datatype_name.clone(),
                defining_id: origin.package.into(),
            })
            .collect();

        Some(type_origins)
    }

    /// BCS representation of the package's modules.  Modules appear as a sequence of pairs (module
    /// name, followed by module bytes), in alphabetic order by module name.
    async fn module_bcs(&self) -> Result<Option<Base64>> {
        let bcs = bcs::to_bytes(self.native.serialized_module_map())
            .map_err(|_| {
                Error::Internal(format!("Failed to serialize package {}", self.native.id()))
            })
            .extend()?;

        Ok(Some(bcs.into()))
    }
}

impl MovePackage {
    fn parsed_package(&self) -> Result<ParsedMovePackage, Error> {
        ParsedMovePackage::read_from_package(&self.native)
            .map_err(|e| Error::Internal(format!("Error reading package: {e}")))
    }

    /// This package was viewed at a snapshot of the chain state at this checkpoint (identified by
    /// its sequence number).
    fn checkpoint_viewed_at_impl(&self) -> u64 {
        self.super_.checkpoint_viewed_at
    }

    pub(crate) fn module_impl(&self, name: &str) -> Result<Option<MoveModule>, Error> {
        use PackageCacheError as E;
        match (
            self.native.serialized_module_map().get(name),
            self.parsed_package()?.module(name),
        ) {
            (Some(native), Ok(parsed)) => Ok(Some(MoveModule {
                storage_id: self.super_.address,
                native: native.clone(),
                parsed: parsed.clone(),
                checkpoint_viewed_at: self.checkpoint_viewed_at_impl(),
            })),

            (None, _) | (_, Err(E::ModuleNotFound(_, _))) => Ok(None),
            (_, Err(e)) => Err(Error::Internal(format!(
                "Unexpected error fetching module: {e}"
            ))),
        }
    }

    pub(crate) async fn query(
        ctx: &Context<'_>,
        address: SuiAddress,
        key: ObjectLookup,
    ) -> Result<Option<Self>, Error> {
        let Some(object) = Object::query(ctx, address, key).await? else {
            return Ok(None);
        };

        Ok(Some(MovePackage::try_from(&object).map_err(|_| {
            Error::Internal(format!("{address} is not a package"))
        })?))
    }
}

impl TryFrom<&Object> for MovePackage {
    type Error = MovePackageDowncastError;

    fn try_from(object: &Object) -> Result<Self, MovePackageDowncastError> {
        let Some(native) = object.native_impl() else {
            return Err(MovePackageDowncastError);
        };

        if let Data::Package(move_package) = &native.data {
            Ok(Self {
                super_: object.clone(),
                native: move_package.clone(),
            })
        } else {
            Err(MovePackageDowncastError)
        }
    }
}
