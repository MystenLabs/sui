// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;
use move_binary_format::CompiledModule;

use crate::{context_data::db_data_provider::PgManager, error::Error};

use super::{move_package::MovePackage, sui_address::SuiAddress};

#[derive(Clone)]
pub(crate) struct MoveModule {
    pub native_module: CompiledModule,
}

/// Represents a module in Move, a library that defines struct types
/// and functions that operate on these types.
#[allow(unreachable_code)]
#[allow(unused_variables)]
#[Object]
impl MoveModule {
    async fn file_format_version(&self) -> u32 {
        self.native_module.version
    }

    // TODO: impl all fields

    // moduleId: MoveModuleId!
    // friends: [MoveModule!]

    // struct(name: String!): MoveStructDecl
    // structConnection(
    //   first: Int,
    //   after: String,
    //   last: Int,
    //   before: String,
    // ): MoveStructConnection

    // function(name: String!): MoveFunction
    // functionConnection(
    //   first: Int,
    //   after: String,
    //   last: Int,
    //   before: String,
    // ): MoveFunctionConnection

    // bytes: Base64
    // disassembly: String
}

#[derive(SimpleObject)]
#[graphql(complex)]
pub(crate) struct MoveModuleId {
    #[graphql(skip)]
    pub package: SuiAddress,
    pub name: String,
}

#[ComplexObject]
impl MoveModuleId {
    /// The package that this Move module was defined in
    async fn package(&self, ctx: &Context<'_>) -> Result<MovePackage> {
        let result = ctx
            .data_unchecked::<PgManager>()
            .fetch_move_package(self.package, None)
            .await
            .extend()?;

        match result {
            Some(result) => Ok(result),
            None => Err(Error::Internal("Package not found".to_string()).extend()),
        }
    }
}
