// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::base64::Base64;
use super::move_module::MoveModule;
use super::object::Object;
use super::sui_address::SuiAddress;
use crate::context_data::db_data_provider::validate_cursor_pagination;
use crate::error::code::INTERNAL_SERVER_ERROR;
use crate::error::{graphql_error, Error};
use async_graphql::connection::{Connection, Edge};
use async_graphql::*;
use move_binary_format::CompiledModule;
use sui_types::{
    move_package::MovePackage as NativeMovePackage, object::Object as NativeSuiObject, Identifier,
};

const DEFAULT_PAGE_SIZE: usize = 10;

#[derive(Clone)]
pub(crate) struct MovePackage {
    pub native_object: NativeSuiObject,
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

#[allow(unreachable_code)]
#[allow(unused_variables)]
#[Object]
impl MovePackage {
    /// A representation of the module called `name` in this package, including the
    /// structs and functions it defines.
    async fn module(&self, name: String) -> Result<Option<MoveModule>> {
        let identifier = Identifier::new(name).map_err(|e| Error::Internal(e.to_string()))?;

        let module = self.native_object.data.try_as_package().map(|x| {
            x.deserialize_module(
                &identifier,
                move_binary_format::file_format_common::VERSION_MAX,
                true,
            )
            .map(|x| MoveModule { native_module: x })
        });
        if let Some(modu) = module {
            return Ok(Some(modu?));
        }
        Ok(None)
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
        // TODO: make cursor opaque.
        // for now it same as module name
        validate_cursor_pagination(&first, &after, &last, &before)?;

        if let Some(mod_map) = self
            .native_object
            .data
            .try_as_package()
            .map(|x| x.serialized_module_map())
        {
            if mod_map.is_empty() {
                return Err(graphql_error(
                    INTERNAL_SERVER_ERROR,
                    format!(
                        "Published package cannot contain zero modules. Id: {}",
                        self.native_object.id()
                    ),
                )
                .into());
            }

            let mut forward = true;
            let mut count = first.unwrap_or(DEFAULT_PAGE_SIZE as u64);
            count = last.unwrap_or(count);

            let mut mod_list = mod_map
                .clone()
                .into_iter()
                .collect::<Vec<(String, Vec<u8>)>>();

            // ok to unwrap because we know mod_map is not empty
            let mut start = &if last.is_some() {
                forward = false;
                mod_list.last().map(|c| c.0.clone())
            } else {
                mod_list.first().map(|c| c.0.clone())
            }
            .unwrap();

            if let Some(aft) = &after {
                start = aft;
            } else if let Some(bef) = &before {
                start = bef;
                forward = false;
            };

            if !forward {
                mod_list = mod_list.into_iter().rev().collect();
            }

            let mut res: Vec<_> = mod_list
                .iter()
                .skip_while(|(name, _)| name.as_str() != start)
                .skip((after.is_some() || before.is_some()) as usize)
                .take(count as usize)
                .map(|(name, module)| {
                    CompiledModule::deserialize_with_config(
                        module,
                        move_binary_format::file_format_common::VERSION_MAX,
                        true,
                    )
                    .map(|x| (name, MoveModule { native_module: x }))
                })
                .collect::<Result<Vec<_>, _>>()?;
            let mut has_prev_page = &mod_list.first().unwrap().0 != start;
            let mut has_next_page =
                !res.is_empty() && &mod_list.last().unwrap().0 != res.last().unwrap().0;
            if !forward {
                res = res.into_iter().rev().collect();
                has_prev_page = &mod_list.first().unwrap().0 != start;
                has_next_page =
                    !res.is_empty() && &mod_list.last().unwrap().0 != res.first().unwrap().0;
            }

            let mut connection = Connection::new(has_prev_page, has_next_page);

            connection.edges.extend(
                res.into_iter()
                    .map(|(name, module)| Edge::new(name.clone(), module)),
            );
            return Ok(Some(connection));
        }

        Ok(None)
    }

    /// The transitive dependencies of this package.
    async fn linkage(&self) -> Result<Option<Vec<Linkage>>> {
        let linkage = self
            .as_native_package()?
            .linkage_table()
            .iter()
            .map(|(&runtime_id, upgrade_info)| Linkage {
                original_id: runtime_id.into(),
                upgraded_id: upgrade_info.upgraded_id.into(),
                version: upgrade_info.upgraded_version.value(),
            })
            .collect();

        Ok(Some(linkage))
    }

    /// The (previous) versions of this package that introduced its types.
    async fn type_origins(&self) -> Result<Option<Vec<TypeOrigin>>> {
        let type_origins = self
            .as_native_package()?
            .type_origin_table()
            .iter()
            .map(|origin| TypeOrigin {
                module: origin.module_name.clone(),
                struct_: origin.struct_name.clone(),
                defining_id: origin.package.into(),
            })
            .collect();

        Ok(Some(type_origins))
    }

    /// BCS representation of the package's modules.  Modules appear as a sequence of pairs (module
    /// name, followed by module bytes), in alphabetic order by module name.
    async fn bcs(&self) -> Result<Option<Base64>> {
        let modules = self.as_native_package()?.serialized_module_map();

        let bcs = bcs::to_bytes(modules).map_err(|_| {
            Error::Internal(format!(
                "Failed to serialize package {}",
                self.native_object.id(),
            ))
        })?;

        Ok(Some(bcs.into()))
    }

    async fn as_object(&self) -> Option<Object> {
        Some(Object::from(&self.native_object))
    }
}

impl MovePackage {
    fn as_native_package(&self) -> Result<&NativeMovePackage> {
        Ok(self.native_object.data.try_as_package().ok_or_else(|| {
            Error::Internal(format!(
                "Failed to convert native object to move package: {}",
                self.native_object.id(),
            ))
        })?)
    }
}
