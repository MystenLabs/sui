// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0
use anyhow::{anyhow, Result};
use move_binary_format::{
    file_format::CompiledModule,
    normalized::{Function, Type},
};
use move_core_types::{
    account_address::AccountAddress, identifier::Identifier, language_storage::TypeTag,
    transaction_argument::TransactionArgument,
};
use regex::Regex;
use sui_types::{
    base_types::ObjectID,
    error::SuiError,
    object::{Data, Object},
};

const HEX_PREFIX: &str = "0x";
const VECTOR_PREFIX: &str = "0v";
const DOUBLE_COLON: &str = "::";
const VECTOR_LEFT_PAD: &str = "x\"";
const VECTOR_RIGHT_PAD: char = '"';
const STRING_QUOTE: char = '"';

pub struct MoveFunctionComponents {
    pub module_name: Identifier,
    pub function_name: Identifier,
    pub type_tags: Vec<TypeTag>,
    pub object_args: Vec<ObjectID>,
    pub pure_args_serialized: Vec<Vec<u8>>,
}

/// This resolves a plain text move function into its individual components
/// A text function looks like any of the following:
///
/// General case (type and variable aliases):
///     Module::function<T1, T2,...>(arg1, arg2, ARG_ALIAS1, ARG_ALIAS2, ...)
/// Simpler case (only variable alias):
///     Module::function(arg1, arg2, ARG_ALIAS1,...)
/// Simpler case (no aliases):
///     Module::function(arg1, arg2,...)
///
/// The logic checks the expected function signature in the Move module and tries to revolve the provided text into the expected arguments and types
///
/// Definition of terms:
///
/// 1. Module: This is the Move module which must be a VALID_IDENTIFIER
/// 2. function: This is the function to call which must be a VALID_IDENTIFIER
/// 3. T1, T2, ...: these are optional alias for the type/generic args. These aliases are uses so the text is shortet. T1 and T2 just aliases
///    for the actual types which may be longer. For example T1 could represent FastX::Coin::Gas::...
/// 4. Type Alias Map: The actual mappings of the type aliases are provided in an optional map `type_alias_map`.
///    Hence in the previous example the map will contain an entry ("T1", "FastX::Coin::Gas::...") which helps resolve the actual type
///    Type aliases have to be VALID_IDENTIFIER
/// 5. arg1, arg2,..: These are the arguments represented as strings and must either be numbers (8,2944,..), boolean (true/false), addresses (0x...), u8 vectors (0v...)
///    or strings with the quotes escaped ("\ This is a valid string \"). Strings are resolved to u8 vectors.
///    Numbers are converted to the expected type matching the function signature. If the number is too large, an error is returned. For example 432 cannot be a u8.
///    Note vectore are raw ascii bytes prefixed with 0v
/// 6. ARG_ALIAS1, ARG_ALIAS2, ...: Similar to Type Aliases, this allows one represent arguments with simpler names in order to reduce the text length.
///    The mappings have to be defined in `var_alias_map` and must be VALID_IDENTIFIER
///
///    For example instead of calling:
///             Module::function(\"Some really long string here \", 0v374934238942349837423942340234982374532453294324537, \"Some really long string here \")
///    This can be reduced to:
///         Module::function(MY_STRING, MY_VECTOR, MY_STRING)
///
///    With the following additional definition:
///         var_alias_map: {
///                             "MY_STRING": "\"Some really long string here \"",
///                             "MY_VECTOR": "0v374934238942349837423942340234982374532453294324537"
///                         }
/// 7. VALID_IDENTIFIER:
///     A valid identifier consists of an ASCII string which satisfies any of the conditions:
///
///     * The first character is a letter and the remaining characters are letters, digits or
///     underscores.
///     * The first character is an underscore, and there is at least one further letter, digit or
///     underscore.
///
/// In the most general sense, one might need to provide type_alias_map and var_alias_map but this is only for complex functions.
/// Most functions should look like the following: Module::function(a,b,c,f)
///
/// TODO:
///     1. Text input currently does not allow commas and parentheses. Easy fix
///     2. Object args are not checked to match the actual object args expected. Easy fix but requires sending all objects to resolveer
///
pub fn resolve_move_function_text(
    package: Object,
    type_alias_map: std::collections::BTreeMap<String, String>,
    var_alias_map: std::collections::BTreeMap<String, String>,
    full_text: String,
    default_module: Identifier,
) -> Result<MoveFunctionComponents> {
    let (mod_name, fn_name, type_tags, obj_args, pure_args) = driver_inner(
        package,
        type_alias_map,
        var_alias_map,
        full_text,
        default_module,
    )?;
    let pure_args = move_core_types::transaction_argument::convert_txn_args(&pure_args);

    Ok(MoveFunctionComponents {
        module_name: mod_name,
        function_name: fn_name,
        type_tags,
        object_args: obj_args,
        pure_args_serialized: pure_args,
    })
}
#[allow(clippy::type_complexity)]
fn driver_inner(
    package: Object,
    type_alias_map: std::collections::BTreeMap<String, String>,
    var_alias_map: std::collections::BTreeMap<String, String>,
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
    // First derive args from CSV
    let args_str_vec = split_args(arg_group)?;

    // Make the necessary variable tag substitutions
    let mut args_str_vec_subs = vec![];
    for arg in args_str_vec {
        args_str_vec_subs.push(var_alias_map.get(&arg).unwrap_or(&arg).to_owned());
    }

    let (obj_args, pure_args) = parse_args(&mut args_str_vec_subs, expected_fn_sign)?;
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
    // Cant have commas in string too
    // Need to fix
    if (s.matches('(').count() != 1) || (s.matches(')').count() != 1) {
        return Err(anyhow!(
            "Parentheses are not allowed in function args or body"
        ));
    }
    let s = s.trim().to_owned();

    let re = Regex::new(r"\b[^()]+\((.*)\)$").unwrap();

    let matches = re.captures(&s);

    let matches = matches
        .ok_or(anyhow!("Cannot match function syntax"))
        .unwrap();

    // Has to be exactly 2
    if matches.len() != 2 {
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

    let mod_end_idx = s.find(DOUBLE_COLON);

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
    if let Some(tp) = type_aliases {
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

// Objects args have to come first then be followed by pure args
fn parse_args(
    args: &mut Vec<String>,
    function_signature: Function,
) -> Result<(Vec<ObjectID>, Vec<TransactionArgument>)> {
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
    // Try to fit the value given into the value expected
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

        match expected_pure_arg {
            Type::Bool => {
                let transformed_arg = curr_pure_arg.to_ascii_lowercase();
                if !((transformed_arg == "true") || (transformed_arg == "false")) {
                    return Err(anyhow!(
                        "Expected boolean at pos: {} (true/false), found {}",
                        idx,
                        curr_pure_arg
                    ));
                }
            }
            // Use str repr for u nums
            u_num @ Type::U8 | u_num @ Type::U64 | u_num @ Type::U128 => {
                transformed_arg.push_str(&format!("{}", u_num));
            }
            // Address should be 0x... as expected
            Type::Address => (),

            // Support quoted strings, hex num (like address), and maybe.... hyphen (?) separated bytes?
            Type::Vector(t) => {
                // Has to be u8 vector
                if **t != Type::U8 {
                    return Err(anyhow!("Only u8 vectors are allowed pos: {}", idx));
                }
                transformed_arg.clear();
                // Need to pad the first part
                transformed_arg.push_str(VECTOR_LEFT_PAD);

                // Check if quoted string
                if curr_pure_arg.starts_with(STRING_QUOTE)
                    && (curr_pure_arg.chars().nth_back(0).unwrap() == STRING_QUOTE)
                {
                    // Take everything but the first and last quotes
                    let bytes_str = hex::encode(&curr_pure_arg[1..curr_pure_arg.len() - 1]);
                    transformed_arg.push_str(&bytes_str);
                } else {
                    let mut tmp = curr_pure_arg.to_lowercase();
                    // Must be bytes at this point
                    // If it starts with VECTOR_PREFIX, remove the VECTOR_PREFIX
                    if let Some(stripped) = tmp.strip_prefix(VECTOR_PREFIX) {
                        tmp = stripped.to_string();
                    }
                    transformed_arg.push_str(&tmp);
                }
                // Pad the end
                transformed_arg.push(VECTOR_RIGHT_PAD);
            }
            _ => return Err(anyhow!("Unexpected arg pos: {}", idx)),
        };

        // We now have a hopefully conformant arg
        // Next is to try parsing it into the actual type
        let p = move_core_types::parser::parse_transaction_argument(&transformed_arg);
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
    for arg in args {
        // Must all be addresses
        let mut arg = arg.to_lowercase();
        if !arg.starts_with(HEX_PREFIX) {
            arg = format!("{}{}", HEX_PREFIX, arg);
        }

        v.push(AccountAddress::from_hex_literal(&arg)?);

        // TODO: extened Objects must match the type of the function signature
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
