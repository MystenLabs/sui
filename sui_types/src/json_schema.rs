use schemars::gen::SchemaGenerator;
use schemars::schema::Schema;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;

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
            Vector(#[schemars(with = "TypeTag")] Box<TypeTag>),
            Struct(#[schemars(with = "StructTag")] StructTag),
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

#[derive(Deserialize, Serialize, JsonSchema)]
pub struct Base64(String);

#[derive(Deserialize, Serialize, JsonSchema)]
pub struct Hex(String);
