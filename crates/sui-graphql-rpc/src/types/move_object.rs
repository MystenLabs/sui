// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::move_value::MoveValue;
use super::{coin::Coin, object::Object};
use crate::context_data::db_data_provider::PgManager;
use crate::types::staked_sui::StakedSui;
use async_graphql::Error;
use async_graphql::*;
use futures::executor::block_on;
use move_binary_format::CompiledModule;
use move_bytecode_utils::{layout::TypeLayoutBuilder, module_cache::GetModule};
use move_core_types::language_storage::ModuleId;
use move_core_types::language_storage::TypeTag;
use move_core_types::resolver::ModuleResolver;
use sui_types::object::Object as NativeSuiObject;

#[derive(Clone)]
pub(crate) struct MoveObject {
    pub native_object: NativeSuiObject,
    pub pg_manager: &'static PgManager,
}

#[allow(unreachable_code)]
#[allow(unused_variables)]
#[Object]
impl MoveObject {
    // TODO: This depends on having a module resolver so make more efficient
    async fn contents(&self) -> Result<Option<MoveValue>, Error> {
        if let Some(struct_tag) = self.native_object.data.struct_tag() {
            let type_tag = TypeTag::Struct(Box::new(struct_tag));
            let type_layout = TypeLayoutBuilder::build_with_types(&type_tag, &self)
                .map_err(|e| Error::new(e.to_string()))?;

            return Ok(Some(MoveValue::new(
                type_layout,
                self.native_object
                    .data
                    .try_as_move()
                    .unwrap()
                    .contents()
                    .into(),
            )));
        }

        Ok(None)
    }

    async fn has_public_transfer(&self) -> Option<bool> {
        self.native_object
            .data
            .try_as_move()
            .map(|x| x.has_public_transfer())
    }

    async fn as_object(&self) -> Option<Object> {
        Some(Object::from(&self.native_object))
    }

    async fn as_coin(&self) -> Option<Coin> {
        self.native_object.data.try_as_move().and_then(|x| {
            if x.is_coin() {
                Some(Coin {
                    move_obj: self.clone(),
                })
            } else {
                None
            }
        })
    }

    async fn as_staked_sui(&self) -> Option<StakedSui> {
        self.native_object.data.try_as_move().and_then(|x| {
            if x.type_().is_staked_sui() {
                Some(StakedSui {
                    move_obj: self.clone(),
                })
            } else {
                None
            }
        })
    }
}

impl GetModule for MoveObject {
    type Error = Error;
    type Item = CompiledModule;

    /// TODO: cache modules
    #[allow(clippy::disallowed_methods)]
    fn get_module_by_id(&self, id: &ModuleId) -> Result<Option<Self::Item>, Self::Error> {
        let p = block_on(self.pg_manager.fetch_native_package(id.address().to_vec()))?;
        p.deserialize_module(
            &id.name().to_owned(),
            move_binary_format::file_format_common::VERSION_MAX,
            true,
        )
        .map_err(|e| Error::new(e.to_string()))
        .map(Some)
    }
}

impl ModuleResolver for MoveObject {
    type Error = Error;

    #[allow(clippy::disallowed_methods)]
    fn get_module(&self, id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        let p = block_on(self.pg_manager.fetch_native_package(id.address().to_vec()))?;
        Ok(p.get_module(id).cloned())
    }
}
