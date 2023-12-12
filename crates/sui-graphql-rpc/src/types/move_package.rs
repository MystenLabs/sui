// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::base64::Base64;
use super::move_module::MoveModule;
use super::object::Object;
use super::sui_address::SuiAddress;
use crate::config::ServiceConfig;
use crate::context_data::db_data_provider::validate_cursor_pagination;
use crate::error::Error;
use async_graphql::connection::{Connection, Edge};
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

#[Object]
impl MovePackage {
    /// A representation of the module called `name` in this package, including the
    /// structs and functions it defines.
    async fn module(&self, name: String) -> Result<Option<MoveModule>> {
        self.module_impl(&name).extend()
    }

    /// Paginate through the MoveModules defined in this package.
    pub async fn module_connection(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
    ) -> Result<Option<Connection<String, MoveModule>>> {
        use std::ops::Bound as B;

        let default_page_size = ctx
            .data::<ServiceConfig>()
            .map_err(|_| Error::Internal("Unable to fetch service configuration.".to_string()))
            .extend()?
            .limits
            .default_page_size;

        // TODO: make cursor opaque.
        // for now it same as module name
        validate_cursor_pagination(&first, &after, &last, &before).extend()?;

        let parsed = self.parsed_package()?;
        let module_range = parsed.modules().range((
            after.map_or(B::Unbounded, B::Excluded),
            before.map_or(B::Unbounded, B::Excluded),
        ));

        let total = module_range.clone().count() as u64;
        let (skip, take) = match (first, last) {
            (Some(first), Some(last)) if last < first => (first - last, last),
            (Some(first), _) => (0, first),
            (None, Some(last)) if last < total => (total - last, last),
            (None, _) => (0, default_page_size),
        };

        let mut connection = Connection::new(false, false);
        for (name, parsed) in module_range.skip(skip as usize).take(take as usize) {
            let Some(native) = self.native.serialized_module_map().get(name) else {
                return Err(Error::Internal(format!(
                    "Module '{name}' exists in PackageCache but not in serialized map.",
                ))
                .extend());
            };

            connection.edges.push(Edge::new(
                name.clone(),
                MoveModule {
                    storage_id: self.super_.address,
                    native: native.clone(),
                    parsed: parsed.clone(),
                },
            ))
        }

        connection.has_previous_page = connection.edges.first().is_some_and(|fst| {
            parsed
                .modules()
                .range::<String, _>((B::Unbounded, B::Excluded(&fst.cursor)))
                .next()
                .is_some()
        });

        connection.has_next_page = connection.edges.last().is_some_and(|lst| {
            parsed
                .modules()
                .range::<String, _>((B::Excluded(&lst.cursor), B::Unbounded))
                .next()
                .is_some()
        });

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
