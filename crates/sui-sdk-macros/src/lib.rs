use proc_macro::TokenStream;
use quote::quote;
use reqwest::header::CONTENT_TYPE;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::str::FromStr;
use sui_sdk::rpc_types::{SuiMoveNormalizedModule, SuiMoveNormalizedType};
use sui_types::base_types::ObjectID;
use syn::{parse_macro_input, AttributeArgs, Lit, Meta, NestedMeta};

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
        .json::<JsonRpcResponse<BTreeMap<String, SuiMoveNormalizedModule>>>()
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
        let struct_tokens = module.structs.iter().map(|(name, move_struct)| {
            let struct_ident = syn::Ident::new(name, proc_macro2::Span::call_site());
            let field_tokens = move_struct.fields.iter().map(|field| {
                let field_ident = syn::Ident::new(
                    &escape_keyword(field.name.clone()),
                    proc_macro2::Span::call_site(),
                );
                let field_type: syn::Type =
                    syn::parse_str(&to_rust_type(&package, module_name, &field.type_)).unwrap();
                quote! {
                    pub #field_ident: #field_type,
                }
            });
            quote! {
                #[derive(serde::Deserialize, Debug)]
                pub struct #struct_ident {
                    #(#field_tokens)*
                }

            }
        });

        quote! {
            pub mod #module_ident{
                use super::*;
                #(#struct_tokens)*
            }
        }
    });

    let package_ident = syn::Ident::new(&package_alias, proc_macro2::Span::call_site());
    let expanded = quote! {
        pub mod #package_ident{
            use super::*;
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
        .json(&json! ({
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

fn to_rust_type(
    own_package: &str,
    current_module: &str,
    move_type: &SuiMoveNormalizedType,
) -> String {
    match move_type {
        SuiMoveNormalizedType::Bool => "bool".to_string(),
        SuiMoveNormalizedType::U8 => "u8".to_string(),
        SuiMoveNormalizedType::U16 => "u16".to_string(),
        SuiMoveNormalizedType::U32 => "u32".to_string(),
        SuiMoveNormalizedType::U64 => "u64".to_string(),
        SuiMoveNormalizedType::U128 => "u128".to_string(),
        SuiMoveNormalizedType::U256 => "u256".to_string(),
        SuiMoveNormalizedType::Address => "sui_types::base_types::SuiAddress".to_string(),
        SuiMoveNormalizedType::Signer => "sui_types::base_types::SuiAddress".to_string(),
        t @ SuiMoveNormalizedType::Struct { .. } => {
            try_resolve_known_types(own_package, current_module, t)
        }
        SuiMoveNormalizedType::Vector(t) => {
            format!("Vec<{}>", to_rust_type(own_package, current_module, t))
        }
        SuiMoveNormalizedType::TypeParameter(_) => "Vec<String>".to_string(),
        SuiMoveNormalizedType::Reference(t) => {
            format!("&{}", to_rust_type(own_package, current_module, t))
        }
        SuiMoveNormalizedType::MutableReference(t) => {
            format!("&mut{}", to_rust_type(own_package, current_module, t))
        }
    }
}

fn try_resolve_known_types(
    own_package: &str,
    current_module: &str,
    move_type: &SuiMoveNormalizedType,
) -> String {
    if let SuiMoveNormalizedType::Struct {
        address,
        module,
        name,
        type_arguments,
    } = move_type
    {
        // normalise address
        let address = ObjectID::from_str(address).unwrap().to_hex_literal();
        let own_package = ObjectID::from_str(own_package).unwrap().to_hex_literal();

        match format!("{address}::{module}::{name}").as_str() {
            "0x2::object::UID" => "sui_types::id::UID".to_string(),
            "0x2::object::ID" => "sui_types::id::ID".to_string(),
            "0x2::versioned::Versioned" => "sui_types::versioned::Versioned".to_string(),
            "0x2::bag::Bag" => "sui_types::collection_types::Bag".to_string(),
            "0x1::type_name::TypeName" => "String".to_string(),
            "0x2::object_bag::ObjectBag" => "sui_types::collection_types::Bag".to_string(),
            "0x2::package::UpgradeCap" => "sui_types::move_package::UpgradeCap".to_string(),
            "0x2::coin::Coin" => "sui_types::coin::Coin".to_string(),
            "0x2::balance::Balance" => "sui_types::balance::Balance".to_string(),
            "0x2::table::Table" => "sui_types::collection_types::Table".to_string(),
            "0x2::vec_map::VecMap" => format!(
                "sui_types::collection_types::VecMap<{},{}>",
                to_rust_type(&own_package, current_module, &type_arguments[0]),
                to_rust_type(&own_package, current_module, &type_arguments[1])
            ),
            "0x2::linked_table::LinkedTable" => {
                format!(
                    "sui_types::collection_types::LinkedTable<{}>",
                    to_rust_type(&own_package, current_module, &type_arguments[0])
                )
            }
            "0x1::option::Option" => {
                format!(
                    "Option<{}>",
                    to_rust_type(&own_package, current_module, &type_arguments[0])
                )
            }
            _ if own_package == address && current_module != module => {
                format!("{module}::{name}")
            }
            _ => {
                format!("{name}")
            }
        }
    } else {
        unreachable!()
    }
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
