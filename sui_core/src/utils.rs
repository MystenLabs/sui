use std::fmt;

// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0
use move_core_types::{
    account_address::AccountAddress, identifier::Identifier, language_storage::TypeTag,
    transaction_argument::TransactionArgument,
};

use sui_types::{
    base_types::ObjectID,
    error::SuiError,
    object::{Data, Object},
};

use anyhow::{anyhow, Result};
use move_binary_format::{
    file_format::CompiledModule,
    normalized::{Function, Type},
};

/*
1. Package objects id: has to be valid hex (16 or 20 bytes) and valid object has to exist
2. Module: valid ident which exists in package
3. Function: valid ident which exists in module
4. Type Args: ordered args which must be transformable to function signature type/generic args
5.

Types: u8, u64, u128, bool, address, u8 vec

fs::write<T, V>(5, "config", true, 482, [1,2,3,4,1])

0. Strip leading and lagging spaces
1. Find first parens
2. Find type args
3.

NO_RIGHT_PAREN


*/
use regex::Regex;

// fn tokenize_method(s: String) -> Vec<String> {
//     let mut er_str = "";

//     // Trim leading and lagging spaces
//     let s = s.trim();

//     if s.is_empty() {
//         panic!("NO_METHOD_BODY")
//     }

//     // R_PAREN has to be last idx
//     if !s.ends_with(')') {
//         panic!("NO_RIGHT_PAREN")
//     }

//     // Find find L_PAREN
//     let l_paren_idx = s.find('(');
//     let l_paren_idx = match l_paren_idx {
//         Some(l) => l,
//         None => panic!("NO_LEFT_PAREN"),
//     };

//     // Now we have the function args
//     let fn_args = &s[l_paren_idx..];

//     // We can get the function module, name, types
//     let fn_qualifiers = &s[..l_paren_idx];

//     // Lets check that the qualifiers make sense
//     // Sample: Module::function_name<Type1, Type2>

//     vec![]
// }

pub fn resolve_move_function_text(
    package: Object,
    type_alias_map: std::collections::BTreeMap<String, String>,
    full_text: String,
    default_module: Identifier,
) -> Result<(
    Identifier,
    Identifier,
    Vec<TypeTag>,
    Vec<AccountAddress>,
    Vec<Vec<u8>>,
)> {
    let (mod_name, fn_name, type_tags, obj_args, pure_args) =
        driver_inner(package, type_alias_map, full_text, default_module)?;

    println!("Module name: {}", mod_name);
    println!("Func name: {}", fn_name);
    println!("Type tags : {:?}", type_tags);
    println!("Object args: {:?}", obj_args);
    println!("Pure args: {:?}", pure_args);

    let pure_args = move_core_types::transaction_argument::convert_txn_args(&pure_args);
    Ok((mod_name, fn_name, type_tags, obj_args, pure_args))
}

fn driver_inner(
    package: Object,
    type_alias_map: std::collections::BTreeMap<String, String>,
    full_text: String,
    default_module: Identifier,
) -> Result<(
    Identifier,
    Identifier,
    Vec<TypeTag>,
    Vec<AccountAddress>,
    Vec<TransactionArgument>,
)> {
    // First split into args and function info
    let (fn_group, arg_group) = split_fn_and_args(full_text)?;

    // Get the module, function, type aliases
    let (mod_name, fn_name, ty_aliases) = split_function_qualifiers(fn_group)?;

    // If no module name, assume default_module
    let mod_name = mod_name.unwrap_or(default_module);
    // Resolve the type aliases
    if ty_aliases.len() != type_alias_map.len() {
        return Err(anyhow!("Types aliases do not match alias map"));
    }
    let mut type_tags = vec![];
    for alias in ty_aliases {
        let ty_tag = type_alias_map.get(&alias.clone().into_string());
        if ty_tag.is_none() {
            return Err(anyhow!("Type alias {} not in alias map", alias));
        }
        let parsed_type_tag = move_core_types::parser::parse_type_tag(ty_tag.unwrap())?;
        type_tags.push(parsed_type_tag)
    }

    // We now have the module, function, and potentially type args
    // Need to fetch the actual package and find the function
    let expected_fn_sign = get_expected_fn_signature(package, mod_name.clone(), fn_name.clone())?;

    // Now we need to verify the signature
    // First turn arg group into CSV
    let mut args_str_vec = split_args(arg_group)?;
    let (obj_args, pure_args) = parse_args(&mut args_str_vec, expected_fn_sign)?;

    // All done
    Ok((mod_name, fn_name, type_tags, obj_args, pure_args))
}
// Get the expected function signature from the package
fn get_expected_fn_signature(
    package_obj: Object,
    module_name: Identifier,
    function_name: Identifier,
) -> Result<Function> {
    let package_id = package_obj.id();
    let function_signature = match package_obj.data {
        Data::Package(modules) => {
            let bytes = modules.get(module_name.as_str());
            if bytes.is_none() {
                return Err(anyhow!(
                    "Module {} not found in package {} ",
                    module_name,
                    package_id
                ));
            }

            let m = CompiledModule::deserialize(bytes.unwrap()).expect(
                "Unwrap safe because FastX serializes/verifies modules before publishing them",
            );
            Function::new_from_name(&m, &function_name).ok_or(SuiError::FunctionNotFound {
                error: format!(
                    "Could not resolve function '{}' in module {}",
                    function_name,
                    m.self_id()
                ),
            })?
        }
        Data::Move(_) => {
            return Err(anyhow!("Cannot call Move object. Expected module ",));
        }
    };
    Ok(function_signature)
}

// Takes the full text and generates function group (module, function, types) and arg group (pure and object args)
fn split_fn_and_args(s: String) -> Result<(String, String)> {
    // Cant have more than 1 left or right parens
    // Need to fix
    if (s.matches('(').count() != 1) || (s.matches(')').count() != 1) {
        return Err(anyhow!(
            "Parentheses are not allowed in function args or body"
        ));
    }
    let s = s.trim().to_owned();

    let re = Regex::new(r"\b[^()]+\((.*)\)$").unwrap();

    let matches = re.captures(&s);
    println!("{}", s);

    let matches = matches
        .ok_or(anyhow!("Cannot match function syntax"))
        .unwrap();

    // Has to be exactly 2
    if matches.len() != 2 {
        println!("{}", s);
        return Err(anyhow!("Cannot match function syntax"));
    }

    // This is the group of args
    // Safe to unwrap since we have 2 items
    let arg_group = matches.get(1).unwrap();

    let fn_group = &s[0..arg_group.start() - 1];
    let arg_group = &s[arg_group.start()..arg_group.end()];
    Ok((fn_group.to_owned(), arg_group.to_owned()))
}

// Takes the arg group and tokenizes
// Args must not include parentheses: TODO: Support parens in String args
fn split_args(s: String) -> Result<Vec<String>> {
    let s = s.trim().to_owned();
    let re = Regex::new(r"([^,]+\(.+?\))|([^,]+)").unwrap();
    let matches = re.captures_iter(&s);
    let mut args = vec![];

    for c in matches {
        match c.get(0) {
            None => continue,
            Some(q) => args.push(q.as_str().trim().to_owned()),
        }
    }
    Ok(args)
}

// Takes the function group and derives the module, function name, and types
// Function name has to exist at least
fn split_function_qualifiers(
    s: String,
) -> Result<(Option<Identifier>, Identifier, Vec<Identifier>)> {
    let s = s.trim().to_owned();

    let mod_end_idx = s.find("::");

    // If module found, trim string and extract module name
    // Its okay if we dont find a module name
    let (s, module_name) = match mod_end_idx {
        None => (s, None),
        Some(q) => {
            // Extract the module name
            let mod_name = s[..q].trim().to_owned();
            if !is_valid_ident(mod_name.clone()) {
                return Err(anyhow!("Invalid module identifier: {}", mod_name));
            }
            (
                s[q + 2..].to_owned(),
                Some(<Identifier as std::str::FromStr>::from_str(&mod_name)?),
            )
        }
    };

    // Everything else is of the format
    // function<T,V> or just function
    let fn_idx = s.find('<');

    let (function_name, type_aliases) = match fn_idx {
        None => (s, None),
        Some(q) => (
            s[..q].trim().to_owned(),
            Some(s[q + 1..s.len() - 1].trim().to_owned()),
        ),
    };
    if !is_valid_ident(function_name.clone()) {
        return Err(anyhow!("Invalid function identifier: {}", function_name));
    }

    // Extract the type aliases
    let mut type_aliases_verif = vec![];
    // Split the type aliases
    if type_aliases.is_some() {
        let tp = type_aliases.unwrap();
        for w in tp.trim().split(',').map(|q| q.trim().to_owned()) {
            if !is_valid_ident(w.clone()) {
                return Err(anyhow!("Invalid type alias identifier: {}", w));
            }
            type_aliases_verif.push(<Identifier as std::str::FromStr>::from_str(w.as_str())?);
        }
    }

    let function_name = <Identifier as std::str::FromStr>::from_str(&function_name)?;

    Ok((module_name, function_name, type_aliases_verif))
}
fn is_valid_ident(s: String) -> bool {
    move_core_types::identifier::is_valid(s.as_str())
}

// Get type tags from str
// Type str is pulled from alias mapping
fn parse_type_tags(type_args: Vec<String>) -> Result<Vec<TypeTag>> {
    let mut v = vec![];
    for t in type_args {
        v.push(move_core_types::parser::parse_type_tag(&t)?);
    }
    Ok(v)
}

// Objects args have to come first then be followed by pure args
fn parse_args(
    args: &mut Vec<String>,
    function_signature: Function,
) -> Result<(Vec<ObjectID>, Vec<TransactionArgument>)> {
    println!("{:?}", args);

    // Cant return anything
    if !function_signature.return_.is_empty() {
        return Err(anyhow!("Function should return nothing"));
    }
    // Lengths have to match, less one, due to TxContext
    let expected_len = function_signature.parameters.len() - 1;
    if args.len() != expected_len {
        return Err(anyhow!("Param lengths do not match"));
    }
    // Separate into obj and type args
    // Find the first pure/primitive type
    let pure_args_start = function_signature
        .parameters
        .iter()
        .position(is_primitive)
        .unwrap_or(function_signature.parameters.len());

    // Everything to the left of pure args must be object args
    let object_args_str = &args[..pure_args_start];

    // Check that the object args are valid
    let obj_args = match check_object_args(object_args_str) {
        Ok(q) => q,
        Err(e) => return Err(anyhow!("Invalid object args {}", e)),
    };
    let mut pure_args = vec![];

    // Check that the rest are valid
    if pure_args_start >= function_signature.parameters.len() {
        // No pure args
        return Ok((obj_args, pure_args));
    }

    // Start pure args parsing

    for (idx, curr) in args
        .iter()
        .enumerate()
        .take(expected_len)
        .skip(pure_args_start)
    {
        // Check that this arg is convertible to the expected argument
        // Trim the trailing spaces
        let curr_pure_arg = curr.trim().to_owned();
        let expected_pure_arg = &function_signature.parameters[idx];
        if curr_pure_arg.is_empty() {
            return Err(anyhow!("Pure arg at pos: {} cannot be white space", idx));
        }

        let mut transformed_arg = curr_pure_arg.clone();

        let t = match expected_pure_arg {
            Type::Bool => {
                let lower = curr_pure_arg.to_ascii_lowercase();
                if !((lower == "true") || (lower == "false")) {
                    return Err(anyhow!(
                        "Expected boolean at pos: {} (true/false), found {}",
                        idx,
                        curr_pure_arg
                    ));
                }
                lower
            }
            Type::U8 => {
                transformed_arg.push_str("u8");
                transformed_arg
            }
            Type::U64 => {
                transformed_arg.push_str("u64");
                transformed_arg
            }
            Type::U128 => {
                transformed_arg.push_str("u128");
                transformed_arg
            }
            // Address should be 0x... as expected
            Type::Address => transformed_arg,

            // Support quoted strings, hex num (like address), and maybe.... hyphen (?) separated bytes?
            Type::Vector(t) => {
                // Has to be u8 vector
                if **t != Type::U8 {
                    return Err(anyhow!("Only u8 vectors are allowed pos: {}", idx));
                }
                extern crate hex_slice;
                use hex_slice::AsHex;

                transformed_arg.clear();
                // Need to pad the first part
                transformed_arg.push_str("x\"");

                // Check if quoted string
                if curr_pure_arg.starts_with('"')
                    && (curr_pure_arg.chars().nth_back(0).unwrap() == '"')
                {
                    // Take everything but the first and last quotes
                    let bytes_str = format!(
                        "{:X}",
                        curr_pure_arg[1..curr_pure_arg.len() - 1]
                            .as_bytes()
                            .as_hex()
                    );

                    transformed_arg.push_str(&trim_hex_repr(bytes_str));
                } else {
                    let mut tmp = curr_pure_arg.to_lowercase();
                    // Must be bytes at this point
                    // If it starts with 0x, remove the 0x
                    if let Some(stripped) = tmp.strip_prefix("0x") {
                        tmp = stripped.to_string();
                    }
                    transformed_arg.push_str(&tmp);
                }
                // Pad the end
                transformed_arg.push('\"');
                transformed_arg
            }
            _ => return Err(anyhow!("Unexpected arg pos: {}", idx)),
        };

        // We now have a hopefully conformant arg
        // Next is to try parsing it
        let p = move_core_types::parser::parse_transaction_argument(&t);
        println!("{}", t);
        if p.is_err() {
            return Err(anyhow!(
                "Unable to parse arg at pos: {}, err: {:?}",
                idx,
                p.err()
            ));
        }
        pure_args.push(p.unwrap());
    }

    Ok((obj_args, pure_args))
}

fn check_object_args(args: &[String]) -> Result<Vec<ObjectID>> {
    let mut v = vec![];
    // Must all be addresses
    for arg in args {
        v.push(AccountAddress::from_hex_literal(arg)?);
    }
    Ok(v)
}
fn is_primitive(t: &Type) -> bool {
    use Type::*;
    match t {
        Bool | U8 | U64 | U128 | Address => true,
        Vector(inner_t) => is_primitive(inner_t),
        Signer | Struct { .. } | TypeParameter(_) | Reference(_) | MutableReference(_) => false,
    }
}

// Hack because tired
fn trim_hex_repr(s: String) -> String {
    s.replace(" ", "").replace("]", "").replace("[", "")
}
