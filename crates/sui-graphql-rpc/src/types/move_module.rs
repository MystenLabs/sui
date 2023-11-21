// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;
use move_binary_format::binary_views::BinaryIndexedView;
use move_disassembler::disassembler::Disassembler;
use move_ir_types::location::Loc;

use crate::context_data::db_data_provider::PgManager;
use crate::error::Error;
use sui_package_resolver::Module as ParsedMoveModule;

use super::{base64::Base64, move_package::MovePackage, sui_address::SuiAddress};

#[derive(Clone)]
pub(crate) struct MoveModule {
    pub native: Vec<u8>,
    pub parsed: ParsedMoveModule,
}

#[derive(SimpleObject)]
#[graphql(complex)]
pub(crate) struct MoveModuleId {
    #[graphql(skip)]
    pub package: SuiAddress,
    pub name: String,
}

/// Represents a module in Move, a library that defines struct types
/// and functions that operate on these types.
#[Object]
impl MoveModule {
    async fn file_format_version(&self) -> u32 {
        self.parsed.bytecode().version
    }

    // TODO: impl all fields

    async fn module_id(&self) -> MoveModuleId {
        // TODO: Rethink the need for MoveModuleId -- we probably don't need it (MoveModule should
        // expose access to its package).
        let self_id = self.parsed.bytecode().self_id();
        MoveModuleId {
            package: SuiAddress::from(*self_id.address()),
            name: self_id.name().to_string(),
        }
    }

    // friendConnection(
    //   first: Int,
    //   after: String,
    //   last: Int,
    //   before: String
    // ): MoveModuleConnection

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

    /// The Base64 encoded bcs serialization of the module.
    async fn bytes(&self) -> Option<Base64> {
        Some(Base64::from(self.native.clone()))
    }

    /// Textual representation of the module's bytecode.
    async fn disassembly(&self) -> Result<Option<String>> {
        let view = BinaryIndexedView::Module(self.parsed.bytecode());
        Ok(Some(
            Disassembler::from_view(view, Loc::invalid())
                .map_err(|e| Error::Internal(format!("Error creating disassembler: {e}")))
                .extend()?
                .disassemble()
                .map_err(|e| Error::Internal(format!("Error creating disassembly: {e}")))
                .extend()?,
        ))
    }
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
