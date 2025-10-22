// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{anyhow, Result};
use move_binary_format::{
    binary_config::BinaryConfig,
    file_format::{CompiledModule, SignatureToken},
};
use move_core_types::account_address::AccountAddress;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use sui_types::{
    base_types::ObjectID, object::Object, transaction::ProgrammableTransaction,
    type_input::TypeInput,
};
use tracing::{debug, warn};

/// Datatype definition: (address, module, name, formal_type_params).
/// This contains linking information in the binary format.
pub type Datatype = (AccountAddress, String, String, Vec<MoveType>);

/// Custom Move type representation for JSON serialization.
/// This provides the exact type information we want to expose in the JSON output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MoveType {
    Bool,
    U8,
    U16,
    U32,
    U64,
    U128,
    U256,
    Address,
    Vector(Box<MoveType>),
    Datatype(Datatype),
    DatatypeInstantiation(Box<(Datatype, Vec<MoveType>)>),
    Reference(Box<MoveType>),
    MutableReference(Box<MoveType>),
    TypeParameter(u16),
}

/// Function signature information for MoveCall commands in a ProgrammableTransaction.
/// Contains detailed parameter and return type information for each function call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionSignature {
    /// Package ID where the function is defined
    pub package: ObjectID,
    /// Module name containing the function
    pub module: String,
    /// Function name
    pub function: String,
    /// Parameter types
    pub parameters: Vec<MoveType>,
    /// Return types
    pub return_types: Vec<MoveType>,
}

/// Move call information containing extracted function signatures.
/// This provides type information for all commands in the transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoveCallInfo {
    /// Vector of function signatures, one for each command
    /// None for non-MoveCall commands, Some(signature) for MoveCall commands
    pub command_signatures: Vec<Option<FunctionSignature>>,
}

impl MoveCallInfo {
    /// Create MoveCallInfo by extracting function signatures from a ProgrammableTransaction.
    /// Creates a vector with one entry per command, None for non-MoveCall commands.
    pub fn from_transaction(
        ptb: &ProgrammableTransaction,
        object_cache: &BTreeMap<ObjectID, BTreeMap<u64, Object>>,
    ) -> Result<Self> {
        let mut command_signatures = Vec::with_capacity(ptb.commands.len());

        for command in ptb.commands.iter() {
            let signature = if let sui_types::transaction::Command::MoveCall(move_call) = command {
                // Extract function signature from the MoveCall
                match Self::extract_function_signature(move_call, object_cache) {
                    Ok(signature) => {
                        debug!(
                            "Successfully extracted signature for {}::{}::{}",
                            signature.package, signature.module, signature.function
                        );
                        Some(signature)
                    }
                    Err(e) => {
                        warn!(
                            "Failed to extract signature for {}::{}::{}: {}",
                            move_call.package, move_call.module, move_call.function, e
                        );
                        None
                    }
                }
            } else {
                None
            };
            command_signatures.push(signature);
        }

        Ok(MoveCallInfo { command_signatures })
    }

    /// Extract function signature information from a MoveCall command.
    fn extract_function_signature(
        move_call: &sui_types::transaction::ProgrammableMoveCall,
        object_cache: &BTreeMap<ObjectID, BTreeMap<u64, Object>>,
    ) -> Result<FunctionSignature> {
        let package_id = move_call.package;
        let module_name = move_call.module.as_str();
        let function_name = move_call.function.as_str();

        // Find the package in the object cache
        let package_obj = object_cache
            .get(&package_id)
            .and_then(|versions| versions.values().next())
            .ok_or_else(|| anyhow!("Package {} not found in cache", package_id))?;

        // Extract MovePackage from the object
        let move_package = package_obj
            .data
            .try_as_package()
            .ok_or_else(|| anyhow!("Object {} is not a package", package_id))?;

        // Get the module bytecode from the package
        let module_bytes = move_package
            .serialized_module_map()
            .get(module_name)
            .ok_or_else(|| anyhow!("Module {} not found in package {}", module_name, package_id))?;

        // Deserialize the module
        let binary_config = BinaryConfig::standard();
        let compiled_module = CompiledModule::deserialize_with_config(module_bytes, &binary_config)
            .map_err(|e| anyhow!("Failed to deserialize module {}: {}", module_name, e))?;

        // Find the function definition
        let (_, function_def) = compiled_module
            .find_function_def_by_name(function_name)
            .ok_or_else(|| {
                anyhow!(
                    "Function {} not found in module {}::{}",
                    function_name,
                    package_id,
                    module_name
                )
            })?;

        // Get the function handle
        let function_handle = compiled_module.function_handle_at(function_def.function);

        // Get parameter and return signatures
        let param_signature = compiled_module.signature_at(function_handle.parameters);
        let return_signature = compiled_module.signature_at(function_handle.return_);

        // Convert TypeInputs to MoveTypes for signature processing
        let type_arguments_as_move_types: Vec<MoveType> = move_call
            .type_arguments
            .iter()
            .map(Self::type_input_to_move_type)
            .collect();

        // Convert SignatureTokens to MoveTypes
        let parameters = param_signature
            .0
            .iter()
            .map(|token| {
                Self::signature_token_to_move_type(
                    token,
                    &compiled_module,
                    &type_arguments_as_move_types,
                )
            })
            .collect::<Result<Vec<_>>>()?;

        let return_types = return_signature
            .0
            .iter()
            .map(|token| {
                Self::signature_token_to_move_type(
                    token,
                    &compiled_module,
                    &type_arguments_as_move_types,
                )
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(FunctionSignature {
            package: package_id,
            module: module_name.to_string(),
            function: function_name.to_string(),
            parameters,
            return_types,
        })
    }

    /// Convert TypeInput to MoveType.
    /// TypeInput comes from the transaction's type arguments.
    fn type_input_to_move_type(type_input: &TypeInput) -> MoveType {
        match type_input {
            TypeInput::Bool => MoveType::Bool,
            TypeInput::U8 => MoveType::U8,
            TypeInput::U16 => MoveType::U16,
            TypeInput::U32 => MoveType::U32,
            TypeInput::U64 => MoveType::U64,
            TypeInput::U128 => MoveType::U128,
            TypeInput::U256 => MoveType::U256,
            TypeInput::Address => MoveType::Address,
            TypeInput::Signer => MoveType::Address, // Signer is treated as Address
            TypeInput::Vector(element) => {
                MoveType::Vector(Box::new(Self::type_input_to_move_type(element)))
            }
            TypeInput::Struct(struct_input) => {
                let type_params: Vec<MoveType> = struct_input
                    .type_params
                    .iter()
                    .map(Self::type_input_to_move_type)
                    .collect();

                let datatype = (
                    struct_input.address,
                    struct_input.module.clone(),
                    struct_input.name.clone(),
                    vec![], // Empty variants - we ignore enum variants
                );

                if type_params.is_empty() {
                    // Non-generic datatype
                    MoveType::Datatype(datatype)
                } else {
                    // Generic instantiation
                    MoveType::DatatypeInstantiation(Box::new((datatype, type_params)))
                }
            }
        }
    }

    /// Convert SignatureToken to MoveType.
    /// This is the core conversion that bridges Move's type system with our custom representation.
    fn signature_token_to_move_type(
        token: &SignatureToken,
        module: &CompiledModule,
        type_arguments: &[MoveType],
    ) -> Result<MoveType> {
        match token {
            SignatureToken::Bool => Ok(MoveType::Bool),
            SignatureToken::U8 => Ok(MoveType::U8),
            SignatureToken::U16 => Ok(MoveType::U16),
            SignatureToken::U32 => Ok(MoveType::U32),
            SignatureToken::U64 => Ok(MoveType::U64),
            SignatureToken::U128 => Ok(MoveType::U128),
            SignatureToken::U256 => Ok(MoveType::U256),
            SignatureToken::Address => Ok(MoveType::Address),
            SignatureToken::Signer => Ok(MoveType::Address), // Signer is treated as Address
            SignatureToken::Vector(element_type) => {
                let element =
                    Self::signature_token_to_move_type(element_type, module, type_arguments)?;
                Ok(MoveType::Vector(Box::new(element)))
            }
            SignatureToken::Datatype(datatype_handle_idx) => {
                let datatype_handle = module.datatype_handle_at(*datatype_handle_idx);
                let module_handle = module.module_handle_at(datatype_handle.module);
                let address = *module.address_identifier_at(module_handle.address);
                let module_name = module.identifier_at(module_handle.name).to_string();
                let struct_name = module.identifier_at(datatype_handle.name).to_string();

                // Non-generic datatype - empty type params
                let datatype = (address, module_name, struct_name, vec![]);
                Ok(MoveType::Datatype(datatype))
            }
            SignatureToken::DatatypeInstantiation(instantiation) => {
                let (datatype_handle_idx, type_args) = instantiation.as_ref();
                let datatype_handle = module.datatype_handle_at(*datatype_handle_idx);
                let module_handle = module.module_handle_at(datatype_handle.module);
                let address = *module.address_identifier_at(module_handle.address);
                let module_name = module.identifier_at(module_handle.name).to_string();
                let struct_name = module.identifier_at(datatype_handle.name).to_string();

                // Resolve the type arguments
                let resolved_type_args = type_args
                    .iter()
                    .map(|arg| Self::signature_token_to_move_type(arg, module, type_arguments))
                    .collect::<Result<Vec<_>>>()?;

                // Base datatype with empty type params
                let datatype = (address, module_name, struct_name, vec![]);

                // Generic instantiation with resolved type arguments
                Ok(MoveType::DatatypeInstantiation(Box::new((
                    datatype,
                    resolved_type_args,
                ))))
            }
            SignatureToken::Reference(inner) => {
                let inner_type = Self::signature_token_to_move_type(inner, module, type_arguments)?;
                Ok(MoveType::Reference(Box::new(inner_type)))
            }
            SignatureToken::MutableReference(inner) => {
                let inner_type = Self::signature_token_to_move_type(inner, module, type_arguments)?;
                Ok(MoveType::MutableReference(Box::new(inner_type)))
            }
            SignatureToken::TypeParameter(idx) => {
                // Resolve type parameter using the provided type_arguments
                if (*idx as usize) < type_arguments.len() {
                    Ok(type_arguments[*idx as usize].clone())
                } else {
                    // Return TypeParameter variant if not resolved
                    Ok(MoveType::TypeParameter(*idx))
                }
            }
        }
    }
}
