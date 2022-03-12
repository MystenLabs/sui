// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    base_types::{ObjectID, TX_CONTEXT_MODULE_NAME, TX_CONTEXT_STRUCT_NAME},
    error::SuiError,
    object::{Data, Object},
    SUI_FRAMEWORK_ADDRESS,
};
use move_binary_format::{
    file_format::Visibility,
    normalized::{Function, Type},
    CompiledModule,
};
use move_core_types::{
    ident_str,
    identifier::{IdentStr, Identifier},
    language_storage::{ModuleId, StructTag, TypeTag},
};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use std::collections::BTreeMap;

// TODO: robust MovePackage tests
// #[cfg(test)]
// #[path = "unit_tests/move_package.rs"]
// mod base_types_tests;

pub const INIT_FN_NAME: &IdentStr = ident_str!("init");

pub struct TypeCheckSuccess {
    pub module_id: ModuleId,
    pub args: Vec<Vec<u8>>,
    pub by_value_objects: BTreeMap<ObjectID, Object>,
    pub mutable_ref_objects: Vec<Object>,
}

// serde_bytes::ByteBuf is an analog of Vec<u8> with built-in fast serialization.
#[derive(Eq, PartialEq, Debug, Clone, Deserialize, Serialize, Hash)]
pub struct MovePackage {
    id: ObjectID,
    module_map: BTreeMap<String, ByteBuf>,
}

impl MovePackage {
    pub fn serialized_module_map(&self) -> &BTreeMap<String, ByteBuf> {
        &self.module_map
    }

    pub fn from_map(id: ObjectID, module_map: &BTreeMap<String, ByteBuf>) -> Self {
        Self {
            id,
            module_map: module_map.clone(),
        }
    }

    pub fn id(&self) -> ObjectID {
        self.id
    }

    pub fn module_id(&self, module: &Identifier) -> Result<ModuleId, SuiError> {
        let ser =
            self.serialized_module_map()
                .get(module.as_str())
                .ok_or(SuiError::ModuleNotFound {
                    module_name: module.to_string(),
                })?;
        Ok(CompiledModule::deserialize(ser)?.self_id())
    }

    /// Get the function signature for the specified function
    pub fn get_function_signature(
        &self,
        module: &Identifier,
        function: &Identifier,
    ) -> Result<Function, SuiError> {
        let bytes =
            self.serialized_module_map()
                .get(module.as_str())
                .ok_or(SuiError::ModuleNotFound {
                    module_name: module.to_string(),
                })?;
        let m = CompiledModule::deserialize(bytes)
            .expect("Unwrap safe because Sui serializes/verifies modules before publishing them");

        Function::new_from_name(&m, function).ok_or(SuiError::FunctionNotFound {
            error: format!(
                "Could not resolve function '{}' in module {}::{}",
                function,
                self.id(),
                module
            ),
        })
    }

    /// Checks if the specified function is an entry function and returns the function if so
    /// There are specific rules for what can be an entry functions
    /// If not entry functions, it returns Err
    pub fn check_and_get_entry_function(
        &self,
        module: &Identifier,
        function: &Identifier,
    ) -> Result<Function, SuiError> {
        let function_signature = self.get_function_signature(module, function)?;

        // Function has to be public
        if function_signature.visibility != Visibility::Public {
            return Err(SuiError::InvalidFunctionSignature {
                error: "Invoked function must have public visibility".to_string(),
            });
        }

        // Function cannot return a value
        if !function_signature.return_.is_empty() {
            return Err(SuiError::InvalidFunctionSignature {
                error: "Invoked function must not return a value".to_string(),
            });
        }

        // Last arg must be `&mut TxContext`
        let last_param = &function_signature.parameters[function_signature.parameters.len() - 1];
        if !is_param_tx_context(last_param) {
            return Err(SuiError::InvalidFunctionSignature {
                error: format!(
                "Expected last parameter of function signature to be &mut {}::{}::{}, but found {}",
                SUI_FRAMEWORK_ADDRESS, TX_CONTEXT_MODULE_NAME, TX_CONTEXT_STRUCT_NAME, last_param
            ),
            });
        }

        Ok(function_signature)
    }
}

impl From<&Vec<CompiledModule>> for MovePackage {
    fn from(compiled_modules: &Vec<CompiledModule>) -> Self {
        let id = ObjectID::from(*compiled_modules[0].self_id().address());

        MovePackage::from_map(
            id,
            &compiled_modules
                .iter()
                .map(|module| {
                    let mut bytes = Vec::new();
                    module.serialize(&mut bytes).unwrap();
                    (module.self_id().name().to_string(), ByteBuf::from(bytes))
                })
                .collect(),
        )
    }
}

/// - Check that `package_object`, `module` and `function` are valid
/// - Check that the the signature of `function` is well-typed w.r.t `type_args`, `object_args`, and `pure_args`
/// - Return the ID of the resolved module, a vector of BCS encoded arguments to pass to the VM, and a partitioning
/// of the input objects into objects passed by value vs by mutable reference
pub fn resolve_and_type_check(
    package_object: Object,
    module: &Identifier,
    function: &Identifier,
    type_args: &[TypeTag],
    object_args: Vec<Object>,
    mut pure_args: Vec<Vec<u8>>,
) -> Result<TypeCheckSuccess, SuiError> {
    // Resolve the function we are calling
    let (function_signature, module_id) = match package_object.data {
        Data::Package(package) => (
            package.check_and_get_entry_function(module, function)?,
            package.module_id(module)?,
        ),
        Data::Move(_) => {
            return Err(SuiError::ModuleLoadFailure {
                error: "Expected a module object, but found a Move object".to_string(),
            })
        }
    };

    // check arity of type and value arguments
    if function_signature.type_parameters.len() != type_args.len() {
        return Err(SuiError::InvalidFunctionSignature {
            error: format!(
                "Expected {:?} type arguments, but found {:?}",
                function_signature.type_parameters.len(),
                type_args.len()
            ),
        });
    }

    // total number of args is |objects| + |pure_args| + 1 for the the `TxContext` object
    let num_args = object_args.len() + pure_args.len() + 1;
    if function_signature.parameters.len() != num_args {
        return Err(SuiError::InvalidFunctionSignature {
            error: format!(
                "Expected {:?} arguments calling function '{}', but found {:?}",
                function_signature.parameters.len(),
                function,
                num_args
            ),
        });
    }

    // type check object arguments passed in by value and by reference
    let mut args = Vec::new();
    let mut mutable_ref_objects = Vec::new();
    let mut by_value_objects = BTreeMap::new();
    #[cfg(debug_assertions)]
    let mut num_immutable_objects = 0;
    #[cfg(debug_assertions)]
    let num_objects = object_args.len();

    let ty_args: Vec<Type> = type_args.iter().map(|t| Type::from(t.clone())).collect();
    for (idx, object) in object_args.into_iter().enumerate() {
        let mut param_type = function_signature.parameters[idx].clone();
        if !param_type.is_closed() {
            param_type = param_type.subst(&ty_args);
        }
        match &object.data {
            Data::Move(m) => {
                args.push(m.contents().to_vec());
                // check that m.type_ matches the parameter types of the function
                match &param_type {
                    Type::MutableReference(inner_t) => {
                        if object.is_read_only() {
                            return Err(SuiError::TypeError {
                                error: format!(
                                    "Argument {} is expected to be mutable, immutable object found",
                                    idx
                                ),
                            });
                        }
                        type_check_struct(&m.type_, inner_t)?;
                        mutable_ref_objects.push(object);
                    }
                    Type::Reference(inner_t) => {
                        type_check_struct(&m.type_, inner_t)?;
                        #[cfg(debug_assertions)]
                        {
                            num_immutable_objects += 1
                        }
                    }
                    Type::Struct { .. } => {
                        if object.is_shared() {
                            // Forbid passing shared (both mutable and immutable) object by value.
                            // This ensures that shared object cannot be transferred, deleted or wrapped.
                            return Err(SuiError::TypeError {
                                error: format!(
                                    "Shared object cannot be passed by-value, found in argument {}",
                                    idx
                                ),
                            });
                        }
                        type_check_struct(&m.type_, &param_type)?;
                        let res = by_value_objects.insert(object.id(), object);
                        // should always pass due to earlier "no duplicate ID's" check
                        debug_assert!(res.is_none())
                    }
                    t => {
                        return Err(SuiError::TypeError {
                            error: format!(
                                "Found object argument {}, but function expects {}",
                                m.type_, t
                            ),
                        })
                    }
                }
            }
            Data::Package(_) => {
                return Err(SuiError::TypeError {
                    error: format!("Found module argument, but function expects {}", param_type),
                })
            }
        }
    }
    #[cfg(debug_assertions)]
    debug_assert!(
        by_value_objects.len() + mutable_ref_objects.len() + num_immutable_objects == num_objects
    );
    // check that the non-object parameters are primitive types
    for param_type in
        &function_signature.parameters[args.len()..function_signature.parameters.len() - 1]
    {
        if !is_primitive(param_type) {
            return Err(SuiError::TypeError {
                error: format!("Expected primitive type, but found {}", param_type),
            });
        }
    }
    args.append(&mut pure_args);

    Ok(TypeCheckSuccess {
        module_id,
        args,
        by_value_objects,
        mutable_ref_objects,
    })
}

/// Checks for the special TxContext param
pub fn is_param_tx_context(param: &Type) -> bool {
    if let Type::MutableReference(typ) = param {
        match &**typ {
            Type::Struct {
                address,
                module,
                name,
                ..
            } if address == &SUI_FRAMEWORK_ADDRESS
                && module.as_ident_str() == TX_CONTEXT_MODULE_NAME
                && name.as_ident_str() == TX_CONTEXT_STRUCT_NAME =>
            {
                return true
            }
            _ => return false,
        }
    }
    false
}

// TODO: upstream Type::is_primitive in diem
pub fn is_primitive(t: &Type) -> bool {
    use Type::*;
    match t {
        Bool | U8 | U64 | U128 | Address => true,
        Vector(inner_t) => is_primitive(inner_t),
        Signer | Struct { .. } | TypeParameter(_) | Reference(_) | MutableReference(_) => false,
    }
}

fn type_check_struct(arg_type: &StructTag, param_type: &Type) -> Result<(), SuiError> {
    if let Some(param_struct_type) = param_type.clone().into_struct_tag() {
        if arg_type != &param_struct_type {
            Err(SuiError::TypeError {
                error: format!(
                    "Expected argument of type {}, but found type {}",
                    param_struct_type, arg_type
                ),
            })
        } else {
            Ok(())
        }
    } else {
        Err(SuiError::TypeError {
            error: format!(
                "Expected argument of type {}, but found struct type {}",
                param_type, arg_type
            ),
        })
    }
}
/// Checks if this module has a conformant `init`
pub fn module_has_init(module: &CompiledModule) -> bool {
    let function = match Function::new_from_name(module, INIT_FN_NAME) {
        Some(v) => v,
        None => return false,
    };
    if function.visibility != Visibility::Private {
        return false;
    }
    if !function.type_parameters.is_empty() {
        return false;
    }
    if !function.return_.is_empty() {
        return false;
    }
    if function.parameters.len() != 1 {
        return false;
    }
    is_param_tx_context(&function.parameters[0])
}
