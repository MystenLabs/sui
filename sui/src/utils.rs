// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0
use anyhow::{anyhow, Result};
use move_binary_format::{
    file_format::CompiledModule,
    normalized::{Function, Type},
};
use move_core_types::{
    account_address::AccountAddress, identifier::Identifier, language_storage::TypeTag,
};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use std::fs;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use sui_types::{
    base_types::{decode_address_hex, ObjectID, SuiAddress},
    error::SuiError,
    object::{Data, Object},
};
use tracing::log::trace;
pub trait Config
where
    Self: DeserializeOwned + Serialize,
{
    fn read_or_create(path: &Path) -> Result<Self, anyhow::Error> {
        let path_buf = PathBuf::from(path);
        Ok(if path_buf.exists() {
            trace!("Reading config from '{:?}'", path);
            let reader = BufReader::new(File::open(path_buf)?);
            let mut config: Self = serde_json::from_reader(reader)?;
            config.set_config_path(path);
            config
        } else {
            trace!("Config file not found, creating new config '{:?}'", path);
            let new_config = Self::create(path)?;
            new_config.write(path)?;
            new_config
        })
    }

    fn write(&self, path: &Path) -> Result<(), anyhow::Error> {
        trace!("Writing config to '{:?}'", path);
        let config = serde_json::to_string_pretty(self).unwrap();
        fs::write(path, config).expect("Unable to write to config file");
        Ok(())
    }

    fn save(&self) -> Result<(), anyhow::Error> {
        self.write(self.config_path())
    }

    fn create(path: &Path) -> Result<Self, anyhow::Error>;

    fn set_config_path(&mut self, path: &Path);
    fn config_path(&self) -> &Path;
}

const HEX_PREFIX: &str = "0x";
const SUI_ADDRESS_LENGTH: usize = AccountAddress::LENGTH;

#[derive(Debug)]
pub struct MoveFunctionComponents {
    pub module: Identifier,
    pub function: Identifier,
    pub type_args: Vec<TypeTag>,
    pub object_args: Vec<ObjectID>,
    pub pure_args_serialized: Vec<Vec<u8>>,
}

pub fn resolve_move_function_components(
    package: Object,
    mod_name: String,
    fn_name: String,
    combined_args_json: Vec<Value>,
) -> Result<MoveFunctionComponents> {
    // First check that the function name is a valid identifier
    let function = Identifier::new(fn_name)?;
    // Module name has to be valid identifier too
    let module = Identifier::new(mod_name)?;

    // We then extract the expected function signature
    let function_signature = get_expected_fn_signature(package, module.clone(), function.clone())?;

    // Now we check that the args are proper

    // Must not return anything
    if !function_signature.return_.is_empty() {
        return Err(anyhow!("Function must return nothing"));
    }
    // Lengths have to match, less one, due to TxContext
    let expected_len = function_signature.parameters.len() - 1;
    if combined_args_json.len() != expected_len {
        return Err(anyhow!("Number of arguments not match"));
    }

    // Object args must always precede the pure/primitive args, so extract those first
    // Find the first non-object args, which marks the start of the pure args
    // Find the first pure/primitive type
    let pure_args_start = function_signature
        .parameters
        .iter()
        .position(is_primitive)
        .unwrap_or(function_signature.parameters.len());

    // Everything to the left of pure args must be object args

    // Check that the object args are valid
    let obj_args = check_object_args_json_extra(&combined_args_json, 0, pure_args_start)?;

    // Check that the pure args are valid or can be made valid

    let pure_args_serialized = check_and_serialize_pure_args(
        &combined_args_json,
        pure_args_start,
        expected_len,
        function_signature,
    )?;

    Ok(MoveFunctionComponents {
        module,
        function,
        object_args: obj_args,
        pure_args_serialized,

        // TODO: add checking type args
        type_args: vec![],
    })
}

fn check_and_serialize_pure_args(
    args: &[Value],
    start: usize,
    end_exclusive: usize,
    function_signature: Function,
) -> Result<Vec<Vec<u8>>> {
    // The vector of serialized arguments
    let mut pure_args_serialized = vec![];

    // Iterate through all
    for (idx, curr) in args
        .iter()
        .enumerate()
        .take(end_exclusive - start)
        .skip(start)
    {
        // The arg we got in JSON
        let curr_pure_arg = curr.to_owned();
        // The type the function expects at this position
        let expected_pure_arg_type = &function_signature.parameters[idx];

        //
        match check_and_refine_pure_args_json(&curr_pure_arg, &expected_pure_arg_type) {
            Ok(a) => pure_args_serialized.push(a),
            Err(e) => return Err(anyhow!("Unable to parse arg at pos: {}, err: {:?}", idx, e)),
        }
    }
    Ok(pure_args_serialized)
}

// TODO: check Object types must match the type of the function signature
// Check read/mutable references
// Add support for ObjectID from VecU8
fn check_object_args_json_extra(
    args: &[Value],
    start: usize,
    end_exclusive: usize,
) -> Result<Vec<ObjectID>> {
    // Every elem has to be a string convertible to a ObjectID
    let mut object_args_ids = vec![];
    for (idx, arg) in args
        .iter()
        .enumerate()
        .take(end_exclusive - start)
        .skip(start)
    {
        let transformed = match arg {
            Value::String(s) => {
                let mut s = s.trim().to_lowercase();
                if !s.starts_with(HEX_PREFIX) {
                    s = format!("{}{}", HEX_PREFIX, s);
                }
                ObjectID::from_hex_literal(&s)?
            }
            _ => {
                return Err(anyhow!(
                    "Unable to parse arg {:?} as ObjectID at pos {}. Expected {:?} byte hex string prefixed with 0x.",
                    ObjectID::LENGTH,
                    idx,
                    arg,
                ))
            }
        };

        object_args_ids.push(transformed);
        // TODO: check Object types must match the type of the function signature
        // Check read/mutable references
    }
    Ok(object_args_ids)
}

// TODO:
// Add struct support from String
// Add generic homogenous array support
fn check_and_refine_pure_args_json(curr_val: &Value, expected_type: &Type) -> Result<Vec<u8>> {
    if !is_primitive(expected_type) {
        return Err(anyhow!(
            "Unexpected arg type {:?} not allowed",
            expected_type
        ));
    }
    println!("{}   {}", curr_val, expected_type);
    match (curr_val, expected_type) {
        (Value::Bool(b), Type::Bool) => bcs::to_bytes::<bool>(b),
        (Value::Number(n), Type::U8) => {
            // TODO: There's probably a shorthand for this
            let k = match n.as_u64() {
                Some(q) => {
                    if q < 256 {
                        Some(q as u8)
                    } else {
                        None
                    }
                }
                None => None,
            }
            .ok_or(anyhow!("Expected u8. Found {}", n))
            .unwrap();
            bcs::to_bytes(&k)
        }
        (Value::Number(n), Type::U64) => {
            // TODO: There's probably a shorthand for this
            let k = n
                .as_u64()
                .ok_or(anyhow!("Expected u8. Found {}", n))
                .unwrap();
            println!("{}", k);
            bcs::to_bytes(&k)
        }

        (Value::String(s), Type::U128) => bcs::to_bytes::<u128>(&s.parse::<u128>()?),
        // Address is actally vector u8
        (Value::String(s), Type::Address) => bcs::to_bytes::<SuiAddress>(&address_from_string(s)?),
        (Value::String(s), Type::Vector(t)) => {
            if **t != Type::U8 {
                return Err(anyhow!(
                    "Cannot convert string arg {} to {:?}",
                    curr_val,
                    expected_type
                ));
            }
            let vec = if s.starts_with(HEX_PREFIX) {
                hex::decode(s.trim_start_matches(HEX_PREFIX))?
            } else {
                s.trim_start_matches(HEX_PREFIX).as_bytes().to_vec()
            };

            bcs::to_bytes::<Vec<u8>>(&vec)
        }

        // TODO:
        // Add struct support from String
        // Add generic homogenous array support
        _ => {
            return Err(anyhow!(
                "Unexpected arg {}. Type {:?} not allowed",
                curr_val,
                expected_type
            ))
        }
    }
    .map_err(|_| anyhow!("Unable to parse {} as {:?}", curr_val, expected_type))
}

fn address_from_string(s: &String) -> Result<SuiAddress> {
    let s = s.trim().to_lowercase();
    let v = decode_address_hex(s.trim_start_matches(HEX_PREFIX));
    if v.is_err() {
        return Err(anyhow!(
            "Expected {}byte Address (0x...), found {:?} with err {:?}",
            SUI_ADDRESS_LENGTH,
            s,
            v.err()
        ));
    }
    Ok(v.unwrap())
}

/// Get the expected function signature from the package, module, and identifier
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
            return Err(anyhow!(
                "Cannot call Move object at ID {}. Expected module",
                package_id
            ));
        }
    };
    Ok(function_signature)
}

// TODO: add this to Type::
fn is_primitive(t: &Type) -> bool {
    use Type::*;
    match t {
        Bool | U8 | U64 | U128 | Address => true,
        Vector(inner_t) => is_primitive(inner_t),
        Signer | Struct { .. } | TypeParameter(_) | Reference(_) | MutableReference(_) => false,
    }
}
