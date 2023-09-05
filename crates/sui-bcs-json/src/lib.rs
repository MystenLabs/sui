// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{anyhow, bail, Result};
use move_core_types::u256::U256;
use serde::{ser::SerializeSeq, Deserialize, Serialize};
use serde_json::{Value, Value::Array};
use std::ffi::{c_char, c_int, CStr};
use std::fmt;
use std::mem::ManuallyDrop;
use sui_types::multisig::{MultiSig, MultiSigPublicKey};
use sui_types::{
    base_types::SuiAddress,
    transaction::{TransactionData, TransactionKind},
};

/// Return 0 for success and 1 for failure
///
/// Converts the JSON data into a BCS array.
/// The result points to the address where the new BCS
/// array is stored. Don't forget to deallocate the memory
/// by calling the sui_bcs_json_free_array function.
///
/// # Safety
/// Unsafe function.
#[no_mangle]
pub unsafe extern "C" fn sui_bcs_from_json(
    type_name: *const c_char,
    json_data: *const c_char,
    result: *mut *const u8,
    length: *mut usize,
) -> usize {
    let type_name = match unsafe { CStr::from_ptr(type_name) }.to_str() {
        Ok(type_name) => type_name,
        Err(e) => {
            ffi_helpers::update_last_error(e);
            return 1;
        }
    };

    let json_data = match unsafe { CStr::from_ptr(json_data) }.to_str() {
        Ok(data) => data,
        Err(e) => {
            ffi_helpers::update_last_error(e);
            return 1;
        }
    };

    match internal_bcs_from_json(type_name, json_data) {
        Ok(output) => {
            let mut vec = output;
            vec.shrink_to_fit();
            assert!(vec.len() == vec.capacity());
            let ptr = vec.as_mut_ptr();
            let len = vec.len();
            // Prevent running `vec`'s destructor so we are in complete control
            // of the allocation.
            let _ = ManuallyDrop::new(vec);
            unsafe {
                *result = ptr;
                *length = len;
            }
            0
        }
        Err(e) => {
            ffi_helpers::update_last_error(e);
            1
        }
    }
}

/// Return 0 if the conversion from BCS to JSON is successful, and 1 or 2 for
/// failure. 1 represents a failure from parsing the BCS to JSON, and 2
/// represents an error building the CString from the JSON data.
///
/// The result argument will point to the address where the JSON
/// string is stored. Make sure you release the allocated memory
/// by calling sui_bcs_json_free_string function!
///
/// # Safety
/// Unsafe function.
#[no_mangle]
pub unsafe extern "C" fn sui_bcs_to_json(
    type_name: *const c_char,
    bcs_ptr: *const u8,
    len: usize,
    result: *mut *const c_char,
    pretty: bool,
) -> usize {
    let type_name = match unsafe { CStr::from_ptr(type_name) }.to_str() {
        Ok(type_name) => type_name,
        Err(e) => {
            ffi_helpers::update_last_error(e);
            return 1;
        }
    };

    // we have a pointer to a pointer, thus we first get the address of the pointer
    // to the bcs array data and then reconstruct the slice
    let mut data = unsafe { std::slice::from_raw_parts(bcs_ptr, len) };
    match internal_bcs_to_json(type_name, &mut data, pretty) {
        Ok(res) => {
            let cstr = std::ffi::CString::new(res);
            let cstr = match cstr {
                Ok(c) => c.into_raw(),
                Err(e) => {
                    ffi_helpers::update_last_error(e);
                    return 2;
                }
            };
            unsafe { *result = cstr as *const i8 };
            0
        }
        Err(e) => {
            ffi_helpers::update_last_error(e);
            1
        }
    }
}

/// Frees a Rust-allocated `Vec<u8>`.
#[no_mangle]
pub extern "C" fn sui_bcs_json_free_array(ptr: *const u8, len: usize) {
    unsafe {
        // SAFETY: not going to mutate the value via the pointer, so this is fine
        drop(Vec::from_raw_parts(ptr as *mut u8, len, len));
    }
}

/// Frees a Rust-allocated string.
#[no_mangle]
pub extern "C" fn sui_bcs_json_free_string(pointer: *const c_char) {
    let _ = unsafe { std::ffi::CString::from_raw(pointer as *mut i8) };
}

/// Deserializes the data to given type T, and then serializes it as a JSON value
fn to_json<T: Serialize + for<'a> Deserialize<'a>>(data: &mut &[u8]) -> Result<Value> {
    let bcs: T = bcs::from_bytes(data)?;
    Ok(serde_json::to_value(bcs)?)
}

/// Deserializes the string data to a given type T, and then serializes it as a BCS array
fn to_bcs<T: Serialize + for<'a> Deserialize<'a>>(data: &str) -> Result<Vec<u8>> {
    let bcs: Result<T, _> = serde_json::from_str(data);
    match bcs {
        Ok(bcs) => Ok(bcs::to_bytes(&bcs)?),
        Err(_) => to_bcs_from_string(data),
    }
}

//**************************************************************************************************
// To JSON helper functions
//**************************************************************************************************

fn internal_bcs_to_json_object(typename: &str, data: &mut &[u8]) -> Result<Value> {
    match typename {
        "u8" | "u16" | "u32" | "u64" | "u128" | "u256" | "bool" | "string" => {
            MyValue::simple_deserialize(data, &TypeLayout::build(typename, 0)?)
        }
        "MultiSig" => to_json::<MultiSig>(data),
        "MultiSigPublicKey" => to_json::<MultiSigPublicKey>(data),
        "SuiAddress" => to_json::<SuiAddress>(data),
        "TransactionData" => to_json::<TransactionData>(data),
        "TransactionKind" => to_json::<TransactionKind>(data),
        typename if typename.starts_with("vector") => {
            MyValue::simple_deserialize(data, &TypeLayout::build(typename, 0)?)
        }
        unsupported => bail!("Requested type {} is currently unsupported.", unsupported),
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum MyValue {
    Bool(bool),
    String(String),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    U128(u128),
    U256(U256),
    Vector(Vec<MyValue>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TypeLayout {
    Bool,
    String,
    U8,
    U16,
    U32,
    U64,
    U128,
    U256,
    Vector(Box<TypeLayout>),
}

// clippy complains that the depth parameter is only used in recursion
// so let's allow that
#[allow(clippy::only_used_in_recursion)]
impl TypeLayout {
    fn build(t: &str, depth: u64) -> Result<TypeLayout> {
        Ok(match t {
            "bool" => TypeLayout::Bool,
            "u8" => TypeLayout::U8,
            "u16" => TypeLayout::U16,
            "u32" => TypeLayout::U32,
            "u64" => TypeLayout::U64,
            "u128" => TypeLayout::U128,
            "u256" => TypeLayout::U256,
            "string" => TypeLayout::String,
            typename if t.starts_with("vector") => {
                let inner_type = is_vector(typename).ok_or(anyhow!("We already checked that this should be a vector type, but instead got {typename}"))?;
                TypeLayout::Vector(Box::new(Self::build(inner_type, depth + 1)?))
            }
            unsupported => bail!("Requested type {} is currently unsupported.", unsupported),
        })
    }
}

/// Use a custom serializer/deserializer for handling vector and nested vector
impl MyValue {
    pub fn simple_deserialize(blob: &[u8], ty: &TypeLayout) -> Result<Value> {
        Ok(serde_json::to_value(bcs::from_bytes_seed(ty, blob)?)?)
    }
    pub fn simple_serialize(&self) -> Option<Vec<u8>> {
        bcs::to_bytes(self).ok()
    }
}

impl serde::Serialize for MyValue {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            MyValue::Bool(b) => serializer.serialize_bool(*b),
            MyValue::String(str) => serializer.serialize_str(str),
            MyValue::U8(i) => serializer.serialize_u8(*i),
            MyValue::U16(i) => serializer.serialize_u16(*i),
            MyValue::U32(i) => serializer.serialize_u32(*i),
            MyValue::U64(i) => serializer.serialize_str(&i.to_string()),
            MyValue::U128(i) => serializer.serialize_str(&i.to_string()),
            MyValue::U256(i) => serializer.serialize_str(&i.to_string()),
            MyValue::Vector(v) => {
                let mut ser = serializer.serialize_seq(Some(v.len()))?;
                for val in v {
                    ser.serialize_element(val)?;
                }
                ser.end()
            }
        }
    }
}

impl<'d> serde::de::DeserializeSeed<'d> for &TypeLayout {
    type Value = MyValue;
    fn deserialize<D: serde::de::Deserializer<'d>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        match self {
            TypeLayout::Bool => bool::deserialize(deserializer).map(MyValue::Bool),
            TypeLayout::String => String::deserialize(deserializer).map(MyValue::String),
            TypeLayout::U8 => u8::deserialize(deserializer).map(MyValue::U8),
            TypeLayout::U16 => u16::deserialize(deserializer).map(MyValue::U16),
            TypeLayout::U32 => u32::deserialize(deserializer).map(MyValue::U32),
            TypeLayout::U64 => u64::deserialize(deserializer).map(MyValue::U64),
            TypeLayout::U128 => u128::deserialize(deserializer).map(MyValue::U128),
            TypeLayout::U256 => U256::deserialize(deserializer).map(MyValue::U256),
            TypeLayout::Vector(layout) => Ok(MyValue::Vector(
                deserializer.deserialize_seq(VectorElementVisitor(layout))?,
            )),
        }
    }
}

struct VectorElementVisitor<'a>(&'a TypeLayout);

impl<'d, 'a> serde::de::Visitor<'d> for VectorElementVisitor<'a> {
    type Value = Vec<MyValue>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Vector")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'d>,
    {
        let mut vals = Vec::new();
        while let Some(elem) = seq.next_element_seed(self.0)? {
            vals.push(elem)
        }
        Ok(vals)
    }
}

fn internal_bcs_to_json(typename: &str, data: &mut &[u8], pretty: bool) -> Result<String> {
    match internal_bcs_to_json_object(typename, data) {
        Ok(value) if pretty => Ok(serde_json::to_string_pretty(&value)?),
        Ok(value) => Ok(serde_json::to_string(&value)?),
        Err(e) => bail!("Cannot convert bcs to json string: {e}"),
    }
}

/// Return the inner type if the outer type is a vector
fn is_vector(typename: &str) -> Option<&str> {
    typename.strip_prefix("vector<")?.strip_suffix('>')
}

//**************************************************************************************************
// To BCS helper functions
//**************************************************************************************************

/// Return a BCS array from the given JSON object.
/// The function returns an error if the nested vectors' values do not have consistent types.
fn internal_bcs_from_json_vector(val: &Value, output: &mut Vec<u8>, typename: &str) -> Result<()> {
    match val {
        Array(arr) if typename.starts_with("vector") => {
            let typename = is_vector(typename).ok_or(anyhow!(
                "We already checked that this should be a vector type, but instead got {typename}"
            ))?;
            // bcs standard requires the length of the arrays that are being decoded
            output.insert(output.len(), arr.len() as u8);
            for i in arr {
                internal_bcs_from_json_vector(i, output, typename)?;
            }
        }
        other => output.extend(
            internal_bcs_from_json(typename, &serde_json::to_string(other)?).map_err(|e| {
                anyhow!("Expected a (nested) vector of {typename}, and got {e} instead.")
            })?,
        ),
    }
    Ok(())
}

fn internal_bcs_from_json(typename: &str, data: &str) -> Result<Vec<u8>> {
    let bcs_bytes = match typename {
        "vector" if typename.starts_with("vector") => to_bcs_from_vector(typename, data),
        "u8" | "u16" | "u32" | "u64" | "u128" | "u256" => string_integer_to_bcs(typename, data),
        "bool" => to_bcs::<bool>(data),
        "string" => to_bcs::<String>(data),
        "MultiSig" => to_bcs::<MultiSig>(data),
        "MultiSigPublicKey" => to_bcs::<MultiSigPublicKey>(data),
        "SuiAddress" => to_bcs::<SuiAddress>(data),
        "TransactionData" => to_bcs::<TransactionData>(data),
        "TransactionKind" => to_bcs::<TransactionKind>(data),
        unsupported => bail!("Requested type {} is currently unsupported.", unsupported),
    };

    match bcs_bytes {
        Ok(bcs) => Ok(bcs),
        Err(e) => bail!("Cannot convert json to bcs, {}", e),
    }
}

fn to_bcs_from_vector(typename: &str, data: &str) -> Result<Vec<u8>> {
    let mut output = vec![];
    let typename = is_vector(typename).ok_or(anyhow!(
        "We already checked that this should be a vector type, but instead got {typename}"
    ))?;
    internal_bcs_from_json_vector(&serde_json::from_str(data)?, &mut output, typename)?;
    Ok(output)
}

/// Allow integers to be decoded either from strings ("1214"),
/// JSON strings (""1214""), or numbers (1214).
fn string_integer_to_bcs(typename: &str, data: &str) -> Result<Vec<u8>> {
    // integers can also be a JSON string, so we need to decode it first
    let data = match serde_json::from_str(data) {
        Ok(d) => d,
        Err(_) => data,
    };
    match typename {
        "u8" => match data.parse::<u8>() {
            Ok(u8_data) => Ok(bcs::to_bytes(&u8_data)?),
            Err(_) => Ok(to_bcs::<u8>(data)?),
        },
        "u16" => match data.parse::<u16>() {
            Ok(u16_data) => Ok(bcs::to_bytes(&u16_data)?),
            Err(_) => Ok(to_bcs::<u16>(data)?),
        },
        "u32" => match data.parse::<u32>() {
            Ok(u32_data) => Ok(bcs::to_bytes(&u32_data)?),
            Err(_) => Ok(to_bcs::<u32>(data)?),
        },
        "u64" => match data.parse::<u64>() {
            Ok(u64_data) => Ok(bcs::to_bytes(&u64_data)?),
            Err(_) => Ok(to_bcs::<u64>(data)?),
        },
        "u128" => match data.parse::<u128>() {
            Ok(u128_data) => Ok(bcs::to_bytes(&u128_data)?),
            Err(_) => Ok(to_bcs::<u128>(data)?),
        },
        "u256" => match U256::from_str_radix(data, 10) {
            Ok(u256_data) => Ok(bcs::to_bytes(&u256_data)?),
            Err(_) => Ok(to_bcs::<U256>(data)?),
        },
        other => bail!("Error: expected u8,u16,u32,u64,u128, or u256, but got {other}."),
    }
}

/// Serializes the input string as a BCS array
fn to_bcs_from_string(data: &str) -> Result<Vec<u8>> {
    Ok(bcs::to_bytes(&data)?)
}

//**************************************************************************************************
// Error helper functions
//**************************************************************************************************

/// Get the length of the last error message in bytes when encoded as UTF-8, including the trailing null. This function wraps last_error_length from ffi_helpers crate.
#[no_mangle]
pub extern "C" fn sui_last_error_length() -> c_int {
    ffi_helpers::error_handling::last_error_length()
}

/// Peek at the most recent error and write its error message (Display impl) into the provided buffer as a UTF-8 encoded string.
///
/// This returns the number of bytes written, or -1 if there was an error.
/// This function wraps error_message_utf8 function from ffi_helpers crate.
///
/// # Safety
/// This is an unsafe function
#[no_mangle]
pub unsafe extern "C" fn sui_last_error_message_utf8(buffer: *mut c_char, length: c_int) -> c_int {
    ffi_helpers::error_handling::error_message_utf8(buffer, length)
}

/// Clear the last error message
///
/// # Safety
/// This is an unsafe function
#[no_mangle]
pub unsafe extern "C" fn sui_clear_last_error_message() {
    ffi_helpers::error_handling::clear_last_error()
}

//**************************************************************************************************
// Tests
//**************************************************************************************************
#[cfg(test)]
mod tests {
    use super::*;
    use shared_crypto::intent::{Intent, IntentMessage, PersonalMessage};
    use std::str::FromStr;
    use sui_types::base_types::random_object_ref;
    use sui_types::base_types::SuiAddress;
    use sui_types::crypto::Signature;
    use sui_types::governance::ADD_STAKE_MUL_COIN_FUN_NAME;
    use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
    use sui_types::signature::GenericSignature;
    use sui_types::sui_system_state::SUI_SYSTEM_MODULE_NAME;
    use sui_types::transaction::ObjectArg;
    use sui_types::transaction::{Argument, CallArg, Command};
    use sui_types::SUI_SYSTEM_PACKAGE_ID;

    struct Data {
        expected_bcs: Vec<u8>,
        expected_json_str: String,
        bcs_from_json: Vec<u8>,
        json_from_bcs: String,
    }

    fn helper<T: serde::ser::Serialize>(typename: &str, data: T) -> Data
    where
        T: serde::Serialize,
    {
        let expected_bcs = bcs::to_bytes(&data).unwrap();
        let json_obj = serde_json::to_value(data).unwrap();
        let expected_json_str = serde_json::to_string_pretty(&json_obj).unwrap();
        let bcs_from_json = internal_bcs_from_json(typename, &expected_json_str).unwrap();
        let json_from_bcs =
            internal_bcs_to_json(typename, &mut bcs_from_json.clone().as_slice(), true).unwrap();
        Data {
            expected_bcs,
            expected_json_str,
            bcs_from_json,
            json_from_bcs,
        }
    }

    // TODO: consolidate these two helpers when we have bcs_to_json_vector implemented
    fn helper_vector<T: Serialize>(typename: &str, data: &Vec<T>) -> Data {
        let expected_bcs = bcs::to_bytes(data).unwrap();
        let expected_json_str =
            serde_json::to_string_pretty(&serde_json::to_value(data).unwrap()).unwrap();

        println!("Expected BCS: {:?}", expected_bcs);
        let mut bcs_from_json = vec![];
        internal_bcs_from_json_vector(
            &serde_json::to_value(data).unwrap(),
            &mut bcs_from_json,
            typename,
        )
        .unwrap();
        let json_from_bcs =
            internal_bcs_to_json(typename, &mut expected_bcs.as_slice(), true).unwrap();

        Data {
            expected_bcs,
            expected_json_str,
            bcs_from_json,
            json_from_bcs,
        }
    }

    // example of using this: number = 10u8, number_str = "10", typename = "u8"
    // number = 101234u64, number_str = "\"101234\"", typename = "u64"
    fn helper_test_integers_as_strings<T: Serialize>(number: T, number_str: &str, typename: &str) {
        let bcs = bcs::to_bytes(&number).unwrap();
        // convert bcs to json
        let json_output = internal_bcs_to_json(typename, &mut bcs.as_slice(), true).unwrap();
        assert_eq!(number_str, json_output);
        // convert output json to bcs
        let bcs_from_json = internal_bcs_from_json(typename, &json_output).unwrap();
        assert_eq!(bcs, bcs_from_json);
        // convert a number from a string to bcs
        let bcs_from_str_number = string_integer_to_bcs(typename, number_str).unwrap();
        assert_eq!(bcs, bcs_from_str_number);
    }

    #[test]
    fn test_vector() {
        let data = &vec![vec![10u8, 1u8, 127u8], vec![68u8]];
        let output = helper_vector("vector<vector<u8>>", data);
        assert_eq!(output.expected_bcs, output.bcs_from_json);
        assert_eq!(output.expected_json_str, output.json_from_bcs);

        let data = &vec![1u8, 2u8, 3u8, 4u8];
        let output = helper_vector("vector<u8>", data);
        assert_eq!(output.expected_bcs, output.bcs_from_json);
        assert_eq!(output.expected_json_str, output.json_from_bcs);

        let data = &vec![1u16, 2u16, 3u16, 4u16];
        let output = helper_vector("vector<u16>", data);
        assert_eq!(output.expected_bcs, output.bcs_from_json);
        assert_eq!(output.expected_json_str, output.json_from_bcs);

        let data = &vec![10, 12, 30, 40];
        let output = helper_vector("vector<u32>", data);
        assert_eq!(output.expected_bcs, output.bcs_from_json);
        assert_eq!(output.expected_json_str, output.json_from_bcs);

        let data = &vec![10214124u64, 12251251u64];
        let output = helper_vector("vector<u64>", data);
        let expected = r#"["10214124", "12251251"]"#;
        let json_obj: Value = serde_json::from_str(expected).unwrap();
        let expected_json_str = serde_json::to_string_pretty(&json_obj).unwrap();
        assert_eq!(output.expected_bcs, output.bcs_from_json);
        assert_eq!(expected_json_str, output.json_from_bcs);

        let data = &vec![340_282_366_920_938_463_463_374_607u128];
        let output = helper_vector("vector<u128>", data);
        let expected = r#"["340282366920938463463374607"]"#;
        let json_obj: Value = serde_json::from_str(expected).unwrap();
        let expected_json_str = serde_json::to_string_pretty(&json_obj).unwrap();
        assert_eq!(output.expected_bcs, output.bcs_from_json);
        assert_eq!(expected_json_str, output.json_from_bcs);

        let number_str = "12880124512523626212541252364367345733";
        let number_one = U256::from_str_radix(number_str, 10).unwrap();
        let data = &vec![number_one];
        let output = helper_vector("vector<u256>", data);
        let expected = r#"["12880124512523626212541252364367345733"]"#;
        let json_obj: Value = serde_json::from_str(expected).unwrap();
        let expected_json_str = serde_json::to_string_pretty(&json_obj).unwrap();
        assert_eq!(output.expected_bcs, output.bcs_from_json);
        assert_eq!(expected_json_str, output.json_from_bcs);

        let data = &vec![vec![number_one]];
        let output = helper_vector("vector<vector<u256>>", data);
        let expected = r#"[["12880124512523626212541252364367345733"]]"#;
        let json_obj: Value = serde_json::from_str(expected).unwrap();
        let expected_json_str = serde_json::to_string_pretty(&json_obj).unwrap();
        assert_eq!(output.expected_bcs, output.bcs_from_json);
        assert_eq!(expected_json_str, output.json_from_bcs);
    }

    #[test]
    fn test_bool() {
        let data = true;
        let output = helper("bool", data);
        assert_eq!(output.expected_bcs, output.bcs_from_json);
        assert_eq!(output.expected_json_str, output.json_from_bcs);
    }

    #[test]
    fn test_u8() {
        let number = 16u8;
        let number_str = "16";
        helper_test_integers_as_strings(number, number_str, "u8");
    }

    #[test]
    fn test_u16() {
        let number = 161u16;
        let number_str = "161";
        helper_test_integers_as_strings(number, number_str, "u16");
    }

    #[test]
    fn test_u32() {
        let number = 255u32;
        let number_str = "255";
        helper_test_integers_as_strings(number, number_str, "u32");
    }

    #[test]
    fn test_u64() {
        let number = 12341u64;
        let number_str = "\"12341\"";
        helper_test_integers_as_strings(number, number_str, "u64");
    }

    #[test]
    fn test_u128() {
        let number = 12341u128;
        let number_str = "\"12341\"";
        helper_test_integers_as_strings(number, number_str, "u128");
    }

    #[test]
    fn test_u256() {
        let number_str = "12880124512523626212541252364367345733";
        let number = U256::from_str_radix(number_str, 10).unwrap();
        let number_str = "\"12880124512523626212541252364367345733\"";
        helper_test_integers_as_strings(number, number_str, "u256");
    }

    #[test]
    fn test_string() {
        let input = "a";
        let output = helper("string", input);

        assert_eq!(output.expected_bcs, output.bcs_from_json);
        assert_eq!(output.expected_json_str, output.json_from_bcs);
    }

    #[test]
    fn test_sui_address() {
        let data = SuiAddress::from_str(
            "0xf821d3483fc7725ebafaa5a3d12373d49901bdfce1484f219daa7066a30df77d",
        )
        .unwrap();
        let output = helper("SuiAddress", data);
        assert_eq!(output.expected_bcs, output.bcs_from_json);
        assert_eq!(output.expected_json_str, output.json_from_bcs);
    }

    #[test]
    fn test_multisig_and_multisig_public_key() {
        // these are copied from sui-types/src/unit_tests/multisig_tests.rs
        // for making multisignature objects.
        let keys = sui_types::utils::keys();
        let pk1 = keys[0].public();
        let pk2 = keys[1].public();
        let pk3 = keys[2].public();
        let multisig_pk = MultiSigPublicKey::new(
            vec![pk1.clone(), pk2.clone(), pk3.clone()],
            vec![1, 1, 1],
            2,
        )
        .unwrap();

        let output = helper("MultiSigPublicKey", multisig_pk.clone());
        assert_eq!(output.expected_bcs, output.bcs_from_json);
        assert_eq!(output.expected_json_str, output.json_from_bcs);

        let msg = IntentMessage::new(
            Intent::sui_transaction(),
            PersonalMessage {
                message: "Hello".as_bytes().to_vec(),
            },
        );
        let sig1: GenericSignature = Signature::new_secure(&msg, &keys[0]).into();
        let sig2: GenericSignature = Signature::new_secure(&msg, &keys[1]).into();
        let sig3: GenericSignature = Signature::new_secure(&msg, &keys[2]).into();

        // Any 2 of 3 signatures verifies ok. We are not interesting in veryfing the multisig, but
        // only in encoding it to BCS and JSON, and decoding them.
        let multi_sig1 =
            MultiSig::combine(vec![sig1.clone(), sig2.clone()], multisig_pk.clone()).unwrap();

        let output = helper("MultiSig", multi_sig1);
        assert_eq!(output.expected_bcs, output.bcs_from_json);
        assert_eq!(output.expected_json_str, output.json_from_bcs);

        let multi_sig2 =
            MultiSig::combine(vec![sig1.clone(), sig3.clone()], multisig_pk.clone()).unwrap();

        let output = helper("MultiSig", multi_sig2);
        assert_eq!(output.expected_bcs, output.bcs_from_json);
        assert_eq!(output.expected_json_str, output.json_from_bcs);

        let multi_sig3 =
            MultiSig::combine(vec![sig2.clone(), sig3.clone()], multisig_pk.clone()).unwrap();

        let output = helper("MultiSig", multi_sig3);
        assert_eq!(output.expected_bcs, output.bcs_from_json);
        assert_eq!(output.expected_json_str, output.json_from_bcs);
    }

    #[test]
    fn test_internal_bcs_to_json() {
        let mut ptb = ProgrammableTransactionBuilder::new();
        let split_coint_amount = ptb.pure(1000u64).unwrap(); // note that we need to specify the u64 type
        ptb.command(Command::SplitCoins(
            Argument::GasCoin,
            vec![split_coint_amount],
        ));
        let sender = SuiAddress::ZERO;
        let recipient = SuiAddress::ZERO;
        let argument_address = ptb.pure(recipient).unwrap();
        ptb.command(Command::TransferObjects(
            vec![Argument::Result(0)],
            argument_address,
        ));

        let builder = ptb.finish();
        let gas_budget = 5_000_000;
        let gas_price = 1000;

        let tx_data = TransactionData::new_programmable(
            sender,
            vec![random_object_ref()],
            builder,
            gas_budget,
            gas_price,
        );
        let output = helper("TransactionData", tx_data);
        assert_eq!(output.expected_bcs, output.bcs_from_json);
        assert_eq!(output.expected_json_str, output.json_from_bcs);
    }

    #[test]
    fn test_internal_json_to_bcs() {
        let input_json = include_str!("../test-data/consensus_ptb.json");
        let bcs_bytes = internal_bcs_from_json("TransactionData", input_json).unwrap();

        let expected_bcs_bytes: Vec<u8> = [
            0, 3, 106, 128, 70, 5, 0, 0, 0, 0, 52, 149, 135, 3, 0, 0, 0, 0, 236, 161, 239, 3, 0, 0,
            0, 0, 18, 233, 119, 162, 181, 231, 101, 147, 51, 91, 62, 60, 107, 66, 171, 225, 230,
            180, 173, 79, 250, 192, 41, 145, 22, 66, 162, 64, 198, 247, 41, 123, 1, 54, 136, 10,
            168, 138, 249, 8, 242, 182, 118, 203, 22, 76, 28, 80, 163, 54, 10, 195, 38, 252, 7,
            161, 250, 78, 76, 227, 63, 116, 231, 194, 170, 6, 0, 0, 0, 0, 0, 0, 0, 32, 161, 153,
            247, 153, 248, 106, 182, 111, 16, 123, 44, 209, 67, 56, 98, 41, 36, 87, 184, 75, 31,
            255, 192, 102, 209, 130, 201, 195, 62, 177, 14, 191, 18, 233, 119, 162, 181, 231, 101,
            147, 51, 91, 62, 60, 107, 66, 171, 225, 230, 180, 173, 79, 250, 192, 41, 145, 22, 66,
            162, 64, 198, 247, 41, 123, 232, 3, 0, 0, 0, 0, 0, 0, 64, 75, 76, 0, 0, 0, 0, 0, 0,
        ]
        .into();

        assert_eq!(expected_bcs_bytes, bcs_bytes);
    }

    #[test]
    fn test_ptb_to_internal_bcs_to_json() {
        let mut ptb = ProgrammableTransactionBuilder::new();
        let split_coint_amount = ptb.pure(1000u64).unwrap(); // note that we need to specify the u64 type
        ptb.command(Command::SplitCoins(
            Argument::GasCoin,
            vec![split_coint_amount],
        ));
        let sender = SuiAddress::ZERO;
        let recipient = SuiAddress::ZERO;
        let argument_address = ptb.pure(recipient).unwrap();
        ptb.command(Command::TransferObjects(
            vec![Argument::Result(0)],
            argument_address,
        ));

        let builder = ptb.finish();
        let gas_budget = 5_000_000;
        let gas_price = 1000;

        let tx_data = TransactionData::new_programmable(
            sender,
            vec![random_object_ref()],
            builder,
            gas_budget,
            gas_price,
        );
        let output = helper("TransactionData", tx_data);
        assert_eq!(output.expected_bcs, output.bcs_from_json);
        assert_eq!(output.expected_json_str, output.json_from_bcs);
    }

    #[test]
    fn test_pay_sui_from_internal_json_to_bcs() {
        let amount = 1000u64;
        let sender = SuiAddress::ZERO;
        let validator = SuiAddress::ZERO;
        let gas_budget = 5_000_000;
        let gas_price = 1000;

        let obj_vec = vec![ObjectArg::ImmOrOwnedObject(random_object_ref())];
        let pt = {
            let mut builder = ProgrammableTransactionBuilder::new();
            let arguments = vec![
                builder.input(CallArg::SUI_SYSTEM_MUT).unwrap(),
                builder.make_obj_vec(obj_vec).unwrap(),
                builder
                    .input(CallArg::Pure(bcs::to_bytes(&amount).unwrap()))
                    .unwrap(),
                builder
                    .input(CallArg::Pure(bcs::to_bytes(&validator).unwrap()))
                    .unwrap(),
            ];
            builder.command(Command::move_call(
                SUI_SYSTEM_PACKAGE_ID,
                SUI_SYSTEM_MODULE_NAME.to_owned(),
                ADD_STAKE_MUL_COIN_FUN_NAME.to_owned(),
                vec![],
                arguments,
            ));
            builder.finish()
        };
        let tx_data = TransactionData::new_programmable(
            sender,
            vec![random_object_ref()],
            pt,
            gas_budget,
            gas_price,
        );
        let output = helper("TransactionData", tx_data);
        assert_eq!(output.expected_bcs, output.bcs_from_json);
        assert_eq!(output.expected_json_str, output.json_from_bcs);
    }
}
