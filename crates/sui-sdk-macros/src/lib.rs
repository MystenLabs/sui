use itertools::Itertools;
use move_core_types::account_address::AccountAddress;
use proc_macro::TokenStream;
use quote::quote;
use reqwest::header::CONTENT_TYPE;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::str::FromStr;
use sui_sdk_types::ObjectId;
use syn::{parse_macro_input, AttributeArgs, Lit, Meta, NestedMeta};

const MOVE: &str = "0x0000000000000000000000000000000000000000000000000000000000000001";
const SUI: &str = "0x0000000000000000000000000000000000000000000000000000000000000002";

#[proc_macro]
pub fn move_contract(input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(input as AttributeArgs);

    let mut package_alias = None;
    let mut sui_env = SuiEnv::Mainnet;
    let mut package = None;

    // Parse macro arguments
    for arg in args {
        match arg {
            NestedMeta::Meta(Meta::NameValue(nv)) if nv.path.is_ident("env") => {
                if let Lit::Str(lit) = nv.lit {
                    sui_env = match lit.value().to_lowercase().as_str() {
                        "mainnet" => SuiEnv::Mainnet,
                        "testnet" => SuiEnv::Testnet,
                        _ => SuiEnv::Custom(lit.value()),
                    };
                }
            }
            NestedMeta::Meta(Meta::NameValue(nv)) if nv.path.is_ident("alias") => {
                if let Lit::Str(lit) = nv.lit {
                    package_alias = Some(lit.value());
                }
            }
            NestedMeta::Meta(Meta::NameValue(nv)) if nv.path.is_ident("package") => {
                if let Lit::Str(lit) = nv.lit {
                    let package_input = lit.value();
                    if package_input.contains("@") || package_input.contains(".sui") {
                        package = Some(resolve_mvr_name(package_input))
                    } else {
                        package = Some(lit.value());
                    }
                }
            }
            _ => {}
        }
    }

    let package = match package {
        Some(package) => package,
        None => {
            return syn::Error::new_spanned(
                proc_macro2::TokenStream::new(),
                "Package must be provided (e.g., `package = \"0xb\"`).",
            )
            .to_compile_error()
            .into();
        }
    };

    let rpc_url = match sui_env {
        SuiEnv::Mainnet => "https://rpc.mainnet.sui.io:443".to_string(),
        SuiEnv::Testnet => "https://rpc.testnet.sui.io:443".to_string(),
        SuiEnv::Custom(s) => s,
    };

    let client = reqwest::blocking::Client::new();
    let res = client
        .post(rpc_url)
        .header(CONTENT_TYPE, "application/json")
        .body(format!(
            r#"
                {{
                  "jsonrpc": "2.0",
                  "id": 1,
                  "method": "sui_getNormalizedMoveModulesByPackage",
                  "params": [
                    "{package}"
                  ]
                }}
        "#
        ))
        .send()
        .unwrap();

    let package_data = res
        .json::<JsonRpcResponse<BTreeMap<String, Value>>>()
        .unwrap()
        .result;

    let package_alias = match package_alias {
        Some(name) => name,
        None => {
            return syn::Error::new_spanned(
                proc_macro2::TokenStream::new(),
                "Package name must be provided (e.g., `name = \"MyPackage\"`).",
            )
            .to_compile_error()
            .into();
        }
    };

    let module_tokens = package_data.iter().map(|(module_name, module)| {
        let module_ident = syn::Ident::new(module_name, proc_macro2::Span::call_site());
        let structs = module["structs"].as_object().unwrap();
        let module_address = AccountAddress::from_str(module["address"].as_str().unwrap()).unwrap();

        let mut struct_tokens = structs
            .iter()
            .map(|(name, move_struct)| {
                let type_parameters = move_struct["typeParameters"].as_array().cloned();
                let (type_parameters, phantoms) =
                    type_parameters.iter().flatten().enumerate().fold(
                        (vec![], vec![]),
                        |(mut type_parameters, mut phantoms), (i, v)| {
                            type_parameters.push(syn::Ident::new(
                                &format!("T{i}"),
                                proc_macro2::Span::call_site(),
                            ));
                            if let Some(true) = v["isPhantom"].as_bool() {
                                let name = syn::Ident::new(
                                    &format!("phantom_data_{i}"),
                                    proc_macro2::Span::call_site(),
                                );
                                let type_: syn::Type =
                                    syn::parse_str(&format!("std::marker::PhantomData<T{i}>"))
                                        .unwrap();
                                phantoms.push(quote! {
                                    #name: #type_,
                                })
                            }
                            (type_parameters, phantoms)
                        },
                    );

                let fields = move_struct["fields"].as_array().unwrap();
                let struct_ident = syn::Ident::new(name, proc_macro2::Span::call_site());
                let field_tokens = fields.iter().map(|field| {
                    let field_ident = syn::Ident::new(
                        &escape_keyword(field["name"].as_str().unwrap().to_string()),
                        proc_macro2::Span::call_site(),
                    );
                    let move_type: MoveType =
                        serde_json::from_value(field["type"].clone()).unwrap();
                    let field_type: syn::Type =
                        syn::parse_str(&move_type.to_rust_type(&package, module_name)).unwrap();
                    quote! {
                        pub #field_ident: #field_type,
                    }
                });

                if type_parameters.is_empty() {
                    quote! {
                        #[derive(serde::Deserialize, Debug)]
                        pub struct #struct_ident {
                            #(#field_tokens)*
                        }
                        impl #struct_ident {
                            pub fn type_() -> MoveObjectType {
                                MoveObjectType::from(move_core_types::language_storage::StructTag {
                                    address: PACKAGE_ID,
                                    module: MODULE_NAME.into(),
                                    name: move_core_types::ident_str!(#name).into(),
                                    type_params: vec![],
                                })
                            }
                        }
                    }
                } else {
                    quote! {
                        #[derive(serde::Deserialize, Debug)]
                        pub struct #struct_ident<#(#type_parameters),*> {
                            #(#field_tokens)*
                            #(#phantoms)*
                        }
                        impl <#(#type_parameters),*> #struct_ident<#(#type_parameters),*> {
                            pub fn type_(type_params: Vec<move_core_types::language_storage::TypeTag>) -> MoveObjectType {
                                MoveObjectType::from(move_core_types::language_storage::StructTag {
                                    address: PACKAGE_ID,
                                    module: MODULE_NAME.into(),
                                    name: move_core_types::ident_str!(#name).into(),
                                    type_params,
                                })
                            }
                        }
                    }
                }
            })
            .peekable();

        if struct_tokens.peek().is_none() {
            quote! {}
        } else {
            let addr_byte_ident = module_address.as_slice();
            quote! {
                pub mod #module_ident{
                    use super::*;
                    pub const PACKAGE_ID: AccountAddress = AccountAddress::new([#(#addr_byte_ident),*]);
                    pub const MODULE_NAME: &IdentStr = move_core_types::ident_str!(#module_name);
                    #(#struct_tokens)*
                }
            }
        }
    });

    let package_ident = syn::Ident::new(&package_alias, proc_macro2::Span::call_site());
    let expanded = quote! {
        pub mod #package_ident{
            use super::*;
            use move_core_types::account_address::AccountAddress;
            use move_core_types::identifier::IdentStr;
            use sui_types::base_types::MoveObjectType;
            #(#module_tokens)*
        }
    };
    TokenStream::from(expanded)
}

fn resolve_mvr_name(package: String) -> String {
    let client = reqwest::blocking::Client::new();
    let request = format!(r#"{{packageByName(name:"{package}"){{address}}}}"#);

    let res = client
        .post("https://mvr-rpc.sui-mainnet.mystenlabs.com/graphql")
        .header(CONTENT_TYPE, "application/json")
        .json(&json!({
            "query": request,
            "variables": Value::Null
        }))
        .send()
        .unwrap();
    res.json::<Value>().unwrap()["data"]["packageByName"]["address"]
        .as_str()
        .unwrap()
        .to_string()
}

enum SuiEnv {
    Mainnet,
    Testnet,
    Custom(String),
}

#[allow(dead_code)]
#[derive(Deserialize, Debug)]
struct JsonRpcResponse<T> {
    jsonrpc: String,
    id: u64,
    result: T,
}

fn escape_keyword(mut name: String) -> String {
    match name.as_str() {
        "for" | "ref" => {
            name.push('_');
            name
        }
        _ => name,
    }
}

#[derive(Deserialize, Debug)]
enum MoveType {
    Bool,
    U8,
    U16,
    U32,
    U64,
    U128,
    U256,
    Address,
    Signer,
    Struct {
        address: String,
        module: String,
        name: String,
        #[serde(default, alias = "typeArguments")]
        type_arguments: Vec<MoveType>,
    },
    Vector(Box<MoveType>),
    Reference(Box<MoveType>),
    MutableReference(Box<MoveType>),
    TypeParameter(u16),
}

impl MoveType {
    fn to_rust_type(&self, own_package: &str, current_module: &str) -> String {
        match self {
            MoveType::Bool => "bool".to_string(),
            MoveType::U8 => "u8".to_string(),
            MoveType::U16 => "u16".to_string(),
            MoveType::U32 => "u32".to_string(),
            MoveType::U64 => "u64".to_string(),
            MoveType::U128 => "u128".to_string(),
            MoveType::U256 => "u256".to_string(),
            MoveType::Address => "sui_sdk_types::Address".to_string(),
            MoveType::Signer => "sui_sdk_types::Address".to_string(),
            t @ MoveType::Struct { .. } => t.try_resolve_known_types(own_package, current_module),
            MoveType::Vector(t) => {
                format!("Vec<{}>", t.to_rust_type(own_package, current_module))
            }
            MoveType::Reference(t) => {
                format!("&{}", t.to_rust_type(own_package, current_module))
            }
            MoveType::MutableReference(t) => {
                format!("&mut{}", t.to_rust_type(own_package, current_module))
            }
            MoveType::TypeParameter(index) => format!("T{index}"),
        }
    }

    fn try_resolve_known_types(&self, own_package: &str, current_module: &str) -> String {
        if let MoveType::Struct {
            address,
            module,
            name,
            type_arguments,
        } = self
        {
            // normalise address
            let address = ObjectId::from_str(address).unwrap().to_string();
            let own_package = ObjectId::from_str(own_package).unwrap().to_string();

            match (address.as_str(), module.as_str(), name.as_str()) {
                (MOVE, "type_name", "TypeName") => "String".to_string(),
                (MOVE, "string", "String") => "String".to_string(),
                (MOVE, "ascii", "String") => "String".to_string(),
                (MOVE, "option", "Option") => {
                    format!(
                        "Option<{}>",
                        type_arguments[0].to_rust_type(&own_package, current_module,)
                    )
                }

                (SUI, "object", "UID") => "sui_sdk_types::ObjectId".to_string(),
                (SUI, "object", "ID") => "sui_sdk_types::ObjectId".to_string(),
                (SUI, "versioned", "Versioned") => "sui_types::versioned::Versioned".to_string(),
                (SUI, "bag", "Bag") => "sui_types::collection_types::Bag".to_string(),
                (SUI, "object_bag", "ObjectBag") => "sui_types::collection_types::Bag".to_string(),
                (SUI, "package", "UpgradeCap") => "sui_types::move_package::UpgradeCap".to_string(),
                (SUI, "coin", "Coin") => "sui_types::coin::Coin".to_string(),
                (SUI, "balance", "Balance") => "u64".to_string(),
                (SUI, "table", "Table") => "sui_types::collection_types::Table".to_string(),
                (SUI, "vec_map", "VecMap") => format!(
                    "sui_types::collection_types::VecMap<{},{}>",
                    type_arguments[0].to_rust_type(&own_package, current_module),
                    type_arguments[1].to_rust_type(&own_package, current_module)
                ),
                (SUI, "linked_table", "LinkedTable") => {
                    format!(
                        "sui_types::collection_types::LinkedTable<{}>",
                        type_arguments[0].to_rust_type(&own_package, current_module)
                    )
                }
                _ => {
                    if type_arguments.is_empty() {
                        format!("{module}::{name}")
                    } else {
                        format!(
                            "{module}::{name}<{}>",
                            type_arguments
                                .iter()
                                .map(|ty| ty.to_rust_type(&own_package, current_module))
                                .join(", ")
                        )
                    }
                }
            }
        } else {
            unreachable!()
        }
    }
}
