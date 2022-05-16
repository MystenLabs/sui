// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// This file contains JsonSchema implementation for a few of the move types that is exposed through the GatewayAPI
/// These types are being use by `schemars` to create schema using the `#[schemars(with = "<type>")]` tag.
use crate::readable_serde::encoding;
use crate::readable_serde::Readable;
use schemars::gen::SchemaGenerator;
use schemars::schema::Schema;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use serde_with::serde_as;
use std::ops::Deref;
#[derive(Deserialize, Serialize)]
pub struct StructTag;

impl JsonSchema for StructTag {
    fn schema_name() -> String {
        "StructTag".to_string()
    }

    fn json_schema(gen: &mut SchemaGenerator) -> Schema {
        #[derive(Deserialize, Serialize, JsonSchema)]
        struct StructTag {
            pub address: AccountAddress,
            pub module: Identifier,
            pub name: Identifier,
            pub type_args: Vec<TypeTag>,
        }
        StructTag::json_schema(gen)
    }
}
#[derive(Deserialize, Serialize)]
pub struct TypeTag;

impl JsonSchema for TypeTag {
    fn schema_name() -> String {
        "TypeTag".to_string()
    }

    fn json_schema(gen: &mut SchemaGenerator) -> Schema {
        #[derive(Deserialize, Serialize, JsonSchema)]
        #[serde(rename_all = "camelCase")]
        enum TypeTag {
            Bool,
            U8,
            U64,
            U128,
            Address,
            Signer,
            Vector(Box<TypeTag>),
            Struct(StructTag),
        }
        TypeTag::json_schema(gen)
    }
}

#[derive(Deserialize, Serialize)]
pub struct Identifier;

impl JsonSchema for Identifier {
    fn schema_name() -> String {
        "Identifier".to_string()
    }

    fn json_schema(gen: &mut SchemaGenerator) -> Schema {
        #[derive(Serialize, Deserialize, JsonSchema)]
        struct Identifier(Box<str>);
        Identifier::json_schema(gen)
    }
}
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct AccountAddress(Hex);

#[serde_as]
#[derive(Deserialize, Serialize)]
pub struct Base64(#[serde_as(as = "Readable<encoding::Base64, _>")] pub Vec<u8>);

impl JsonSchema for Base64 {
    fn schema_name() -> String {
        "Base64".to_string()
    }

    fn json_schema(gen: &mut SchemaGenerator) -> Schema {
        #[derive(Serialize, Deserialize, JsonSchema)]
        struct Base64(String);
        Base64::json_schema(gen)
    }
}

impl Deref for Base64 {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Deserialize, Serialize, JsonSchema)]
pub struct Hex(String);

#[derive(Deserialize, Serialize)]
pub struct MoveStructLayout;

impl JsonSchema for MoveStructLayout {
    fn schema_name() -> String {
        "MoveStructLayout".to_string()
    }

    fn json_schema(gen: &mut SchemaGenerator) -> Schema {
        #[derive(Deserialize, Serialize, JsonSchema)]
        enum MoveStructLayout {
            Runtime(Vec<MoveTypeLayout>),
            WithFields(Vec<MoveFieldLayout>),
            WithTypes {
                type_: StructTag,
                fields: Vec<MoveFieldLayout>,
            },
        }
        MoveStructLayout::json_schema(gen)
    }
}

#[derive(Deserialize, Serialize)]
struct MoveTypeLayout;

impl JsonSchema for MoveTypeLayout {
    fn schema_name() -> String {
        "MoveTypeLayout".to_string()
    }

    fn json_schema(gen: &mut SchemaGenerator) -> Schema {
        #[derive(Deserialize, Serialize, JsonSchema)]
        #[serde(rename_all = "camelCase")]
        enum MoveTypeLayout {
            Bool,
            U8,
            U64,
            U128,
            Address,
            Vector(Box<MoveTypeLayout>),
            Struct(MoveStructLayout),
            Signer,
        }
        MoveTypeLayout::json_schema(gen)
    }
}
#[derive(Serialize, Deserialize)]
pub struct MoveFieldLayout;

impl JsonSchema for MoveFieldLayout {
    fn schema_name() -> String {
        "MoveFieldLayout".to_string()
    }

    fn json_schema(gen: &mut SchemaGenerator) -> Schema {
        #[derive(Deserialize, Serialize, JsonSchema)]
        struct MoveFieldLayout {
            name: Identifier,
            layout: MoveTypeLayout,
        }
        MoveFieldLayout::json_schema(gen)
    }
}
