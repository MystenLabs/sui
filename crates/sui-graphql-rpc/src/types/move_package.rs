// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::base64::Base64;
use super::cursor::{Cursor, Page};
use super::move_module::MoveModule;
use super::object::Object;
use super::sui_address::SuiAddress;
use crate::data::Db;
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
    version: u64,
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

pub(crate) type CModule = Cursor<String>;

/// A MovePackage is a kind of Move object that represents code that has been published on chain.
/// It exposes information about its modules, type definitions, functions, and dependencies.
#[Object]
impl MovePackage {
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

        let parsed = self.parsed_package()?;
        let module_range = parsed.modules().range::<String, _>((
            page.after().map_or(B::Unbounded, B::Excluded),
            page.before().map_or(B::Unbounded, B::Excluded),
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

            let cursor = Cursor::new(name.clone()).encode_cursor();
            connection.edges.push(Edge::new(
                cursor,
                MoveModule {
                    storage_id: self.super_.address,
                    native: native.clone(),
                    parsed: parsed.clone(),
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
                version: upgrade_info.upgraded_version.value(),
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
                struct_: origin.struct_name.clone(),
                defining_id: origin.package.into(),
            })
            .collect();

        Some(type_origins)
    }

    /// BCS representation of the package's modules.  Modules appear as a sequence of pairs (module
    /// name, followed by module bytes), in alphabetic order by module name.
    async fn bcs(&self) -> Result<Option<Base64>> {
        let bcs = bcs::to_bytes(self.native.serialized_module_map())
            .map_err(|_| {
                Error::Internal(format!("Failed to serialize package {}", self.native.id()))
            })
            .extend()?;

        Ok(Some(bcs.into()))
    }

    async fn as_object(&self) -> &Object {
        &self.super_
    }
}

impl MovePackage {
    fn parsed_package(&self) -> Result<ParsedMovePackage, Error> {
        // TODO: Leverage the package cache (attempt to read from it, and if that doesn't succeed,
        // write back the parsed Package to the cache as well.)
        ParsedMovePackage::read(&self.super_.native)
            .map_err(|e| Error::Internal(format!("Error reading package: {e}")))
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
            })),

            (None, _) | (_, Err(E::ModuleNotFound(_, _))) => Ok(None),
            (_, Err(e)) => Err(Error::Internal(format!(
                "Unexpected error fetching module: {e}"
            ))),
        }
    }

    pub(crate) async fn query(
        db: &Db,
        address: SuiAddress,
        version: Option<u64>,
    ) -> Result<Option<Self>, Error> {
        let Some(object) = Object::query(db, address, version).await? else {
            return Ok(None);
        };

        Ok(Some(MovePackage::try_from(&object).map_err(|_| {
            Error::Internal(format!("{address} is not a package"))
        })?))
    }
}

impl TryFrom<&Object> for MovePackage {
    type Error = MovePackageDowncastError;

    fn try_from(object: &Object) -> Result<Self, Self::Error> {
        if let Data::Package(move_package) = &object.native.data {
            Ok(Self {
                super_: object.clone(),
                native: move_package.clone(),
            })
        } else {
            Err(MovePackageDowncastError)
        }
    }
}
