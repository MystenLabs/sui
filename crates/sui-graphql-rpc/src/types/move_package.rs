// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::move_module::MoveModule;
use super::object::Object;
use crate::context_data::db_data_provider::validate_cursor_pagination;
use crate::error::code::INTERNAL_SERVER_ERROR;
use crate::error::{graphql_error, Error};
use async_graphql::connection::{Connection, Edge};
use async_graphql::*;
use move_binary_format::CompiledModule;
use sui_types::object::Object as NativeSuiObject;
use sui_types::Identifier;

const DEFAULT_PAGE_SIZE: usize = 10;

#[derive(Clone)]
pub(crate) struct MovePackage {
    pub native_object: NativeSuiObject,
}

#[allow(unreachable_code)]
#[allow(unused_variables)]
#[Object]
impl MovePackage {
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

    async fn as_object(&self) -> Option<Object> {
        Some(Object::from(&self.native_object))
    }
}
