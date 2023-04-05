// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// ! Module defining attributes used by the generator.

use move_model::{
    ast::{Attribute, AttributeValue, Value},
    model::{FunctionEnv, GlobalEnv, ModuleEnv, StructEnv},
};

const CONTRACT_ATTR: &str = "evm_contract";
const STORAGE_ATTR: &str = "storage";
const CREATE_ATTR: &str = "create";
const CALLABLE_ATTR: &str = "callable";
const EVM_ARITH_ATTR: &str = "evm_arith";
const PAYABLE_ATTR: &str = "payable";
const RECEIVE_ATTR: &str = "receive";
const RECEIVE_FALLBACK_ATTR: &str = "fallback";
const EVM_TEST_ATTR: &str = "evm_test";
const TEST_ATTR: &str = "test";
const EXTERNAL_ATTR: &str = "external";
const SIGNATURE: &str = "sig";
const EVENT_ATTR: &str = "event";
const VIEW_ATTR: &str = "view";
const PURE_ATTR: &str = "pure";
const DECODE_ATTR: &str = "decode";
const ENCODE_ATTR: &str = "encode";
const ENCODE_PACKED_ATTR: &str = "encode_packed";
const ABI_STRUCT_ATTR: &str = "abi_struct";

// For async move contracts
const ACTOR_ATTR: &str = "actor";
const STATE_ATTR: &str = "state";
const INIT_ATTR: &str = "init";
const MESSAGE_ATTR: &str = "message";

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) enum FunctionAttribute {
    Payable,
    NonPayable,
    View,
    Pure,
}

/// Extract the value from an attribute
fn extract_attr_value_str(
    env: &GlobalEnv,
    attrs: &[Attribute],
    attr_name: &str,
    value_name: &str,
) -> Option<String> {
    for attr in attrs {
        if let Attribute::Apply(_, s, args) = attr {
            if env.symbol_pool().string(*s).as_str() == attr_name {
                for inner_attr in args {
                    if let Attribute::Assign(_, symbol, value) = inner_attr {
                        if env.symbol_pool().string(*symbol).as_str() == value_name {
                            if let AttributeValue::Value(_, Value::ByteArray(vec_str)) = value {
                                return Some(
                                    vec_str.iter().map(|c| *c as char).collect::<String>(),
                                );
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

/// Extract the solidity signature from the callable attribute
pub fn extract_callable_or_create_signature(
    fun: &FunctionEnv<'_>,
    callable_flag: bool,
) -> Option<String> {
    let attr = if callable_flag {
        CALLABLE_ATTR
    } else {
        CREATE_ATTR
    };
    extract_attr_value_str(fun.module_env.env, fun.get_attributes(), attr, SIGNATURE)
}

/// Extract the solidity signature from the callable attribute
pub fn extract_external_signature(fun: &FunctionEnv<'_>) -> Option<String> {
    extract_attr_value_str(
        fun.module_env.env,
        fun.get_attributes(),
        EXTERNAL_ATTR,
        SIGNATURE,
    )
}

/// Extract the event signature from the event attribute
pub fn extract_event_signature(st: &StructEnv<'_>) -> Option<String> {
    extract_attr_value_str(
        st.module_env.env,
        st.get_attributes(),
        EVENT_ATTR,
        SIGNATURE,
    )
}

/// Extract the solidity signature from the decode attribute
pub fn extract_decode_signature(fun: &FunctionEnv<'_>) -> Option<String> {
    extract_attr_value_str(
        fun.module_env.env,
        fun.get_attributes(),
        DECODE_ATTR,
        SIGNATURE,
    )
}

/// Extract the solidity signature from the encode attribute
pub fn extract_encode_signature(fun: &FunctionEnv<'_>, packed_flag: bool) -> Option<String> {
    let attr = if packed_flag {
        ENCODE_PACKED_ATTR
    } else {
        ENCODE_ATTR
    };
    extract_attr_value_str(fun.module_env.env, fun.get_attributes(), attr, SIGNATURE)
}

/// Extract the contract name.
pub fn extract_contract_name(module: &ModuleEnv<'_>) -> Option<String> {
    extract_attr_value_str(module.env, module.get_attributes(), CONTRACT_ATTR, "name")
}

/// Extract the event signature from the event attribute
pub fn extract_abi_struct_signature(st: &StructEnv<'_>) -> Option<String> {
    extract_attr_value_str(
        st.module_env.env,
        st.get_attributes(),
        ABI_STRUCT_ATTR,
        SIGNATURE,
    )
}

/// Check whether an attribute is present in an attribute list.
pub fn has_attr(env: &GlobalEnv, attrs: &[Attribute], name: &str, simple_flag: bool) -> bool {
    let is_empty = |args: &Vec<Attribute>| {
        if simple_flag {
            args.is_empty()
        } else {
            true
        }
    };
    attrs.iter().any(|a| matches!(a, Attribute::Apply(_, s, args) if is_empty(args) && env.symbol_pool().string(*s).as_str() == name))
}

/// Check whether the module has a `#[evm_contract]` attribute.
pub fn is_evm_contract_module(module: &ModuleEnv) -> bool {
    has_attr(module.env, module.get_attributes(), CONTRACT_ATTR, false)
        || has_attr(module.env, module.get_attributes(), ACTOR_ATTR, false)
}

/// Check whether the module has a `#[evm_arith]` attribute.
pub fn is_evm_arith_module(module: &ModuleEnv) -> bool {
    has_attr(module.env, module.get_attributes(), EVM_ARITH_ATTR, true)
}

/// Check whether the struct has a `#[storage]` attribute.
pub fn is_storage_struct(str: &StructEnv) -> bool {
    has_attr(
        str.module_env.env,
        str.get_attributes(),
        STORAGE_ATTR,
        false,
    ) || has_attr(str.module_env.env, str.get_attributes(), STATE_ATTR, false)
}

/// Check whether the struct has a `#[event]` attribute.
pub fn is_event_struct(str: &StructEnv) -> bool {
    has_attr(str.module_env.env, str.get_attributes(), EVENT_ATTR, false)
}

/// Check whether the function has a `#[callable]` attribute.
pub fn is_callable_fun(fun: &FunctionEnv<'_>) -> bool {
    has_attr(
        fun.module_env.env,
        fun.get_attributes(),
        CALLABLE_ATTR,
        false,
    ) || has_attr(
        fun.module_env.env,
        fun.get_attributes(),
        MESSAGE_ATTR,
        false,
    )
}

/// Check whether the function has a `#[create]` or `#[init]` attribute.
pub fn is_create_fun(fun: &FunctionEnv<'_>) -> bool {
    has_attr(fun.module_env.env, fun.get_attributes(), CREATE_ATTR, false)
        || has_attr(fun.module_env.env, fun.get_attributes(), INIT_ATTR, false)
}

/// Check whether the function has a `#[payable]` attribute.
pub fn is_payable_fun(fun: &FunctionEnv<'_>) -> bool {
    has_attr(fun.module_env.env, fun.get_attributes(), PAYABLE_ATTR, true)
}

/// Check whether the function has a `#[receive]` attribute.
pub fn is_receive_fun(fun: &FunctionEnv<'_>) -> bool {
    has_attr(fun.module_env.env, fun.get_attributes(), RECEIVE_ATTR, true)
}

/// Check whether the function has a `#[fallback]]` attribute.
pub fn is_fallback_fun(fun: &FunctionEnv<'_>) -> bool {
    has_attr(
        fun.module_env.env,
        fun.get_attributes(),
        RECEIVE_FALLBACK_ATTR,
        true,
    )
}

/// Check whether the function has a `#[evm_test] attribute.
pub fn is_evm_test_fun(fun: &FunctionEnv<'_>) -> bool {
    has_attr(
        fun.module_env.env,
        fun.get_attributes(),
        EVM_TEST_ATTR,
        true,
    )
}

/// Check whether the function has a `#[test]` attribute.
pub fn is_test_fun(fun: &FunctionEnv<'_>) -> bool {
    has_attr(fun.module_env.env, fun.get_attributes(), TEST_ATTR, false)
}

/// Check whether the function has a `#[external]` attribute.
pub fn is_external_fun(fun: &FunctionEnv<'_>) -> bool {
    has_attr(
        fun.module_env.env,
        fun.get_attributes(),
        EXTERNAL_ATTR,
        false,
    )
}

pub(crate) fn construct_fun_attribute(fun: &FunctionEnv<'_>) -> Option<FunctionAttribute> {
    let mut res = None;

    for attr in fun.get_attributes() {
        match attr {
            Attribute::Apply(_, name, args) if args.is_empty() => {
                match fun.module_env.env.symbol_pool().string(*name).as_str() {
                    VIEW_ATTR => {
                        if res.is_some() {
                            return None;
                        }
                        res = Some(FunctionAttribute::View);
                    }
                    PURE_ATTR => {
                        if res.is_some() {
                            return None;
                        }
                        res = Some(FunctionAttribute::Pure);
                    }
                    PAYABLE_ATTR => {
                        if res.is_some() {
                            return None;
                        }
                        res = Some(FunctionAttribute::Payable);
                    }
                    _ => (),
                }
            }
            _ => (),
        }
    }

    Some(res.unwrap_or(FunctionAttribute::NonPayable))
}

/// Check whether the function has a `#[decode]` attribute.
pub fn is_decode(fun: &FunctionEnv<'_>) -> bool {
    has_attr(fun.module_env.env, fun.get_attributes(), DECODE_ATTR, false)
}

/// Check whether the function has a `#[encode]` attribute.
pub fn is_encode(fun: &FunctionEnv<'_>) -> bool {
    has_attr(fun.module_env.env, fun.get_attributes(), ENCODE_ATTR, false)
}

/// Check whether the function has a `#[encode_packed]` attribute.
pub fn is_encode_packed(fun: &FunctionEnv<'_>) -> bool {
    has_attr(
        fun.module_env.env,
        fun.get_attributes(),
        ENCODE_PACKED_ATTR,
        false,
    )
}

/// Check whether the function has a `#[abi_struct]` attribute.
pub fn is_abi_struct(st: &StructEnv<'_>) -> bool {
    has_attr(
        st.module_env.env,
        st.get_attributes(),
        ABI_STRUCT_ATTR,
        false,
    )
}
