// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use rand::prelude::*;
use serde::{
    de::{DeserializeSeed, Error, Expected, SeqAccess, Unexpected, Visitor},
    ser::SerializeSeq,
    Deserialize, Deserializer, Serialize, Serializer,
};
use std::ffi::{c_char, c_int, CStr};
use std::fmt;
use sui_sdk2::types::{
    Address, Ed25519PublicKey, InputArgument, MultisigAggregatedSignature as MultiSig,
    MultisigMemberPublicKey as MultiSigPublicKey, Transaction, TransactionKind,
};

/// Converts the JSON data into a BCS array.
/// The result points to the address where the new BCS
/// array is stored. Don't forget to deallocate the memory
/// by calling the sui_bcs_json_free_array function.
///
/// Returns 0 for success, 1 for failing to create the Rust strings from the
/// input pointers and 2 for failing to convert the JSON to BCS.
///
/// # Safety
/// Unsafe function.
#[no_mangle]
pub unsafe extern "C" fn sui_json_to_bcs(
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

    match json_to_bcs(json_data, type_name) {
        Ok(output) => {
            let mut vec = output;
            vec.shrink_to_fit();
            assert!(vec.len() == vec.capacity());
            let slice = vec.leak();
            unsafe {
                *result = slice.as_ptr();
                *length = slice.len();
            }
            0
        }
        Err(e) => {
            ffi_helpers::update_last_error(e);
            2
        }
    }
}

/// Converts the BCS array into a JSON string.
/// The result argument will point to the address where the JSON
/// string is stored. Make sure you release the allocated memory
/// by calling sui_bcs_json_free_string function!
///
/// Returns 0 for success, 1 for failing to create the Rust strings from the
/// input pointers and 2 for failing to convert the BCS array into JSON.
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
    let data = unsafe { std::slice::from_raw_parts(bcs_ptr, len) };
    match bcs_to_json(&data, type_name, pretty) {
        Ok(res) => {
            let cstr = std::ffi::CString::new(res);
            let cstr = match cstr {
                Ok(c) => c.into_raw(),
                Err(e) => {
                    ffi_helpers::update_last_error(e);
                    return 1;
                }
            };
            unsafe { *result = cstr as *const i8 };
            0
        }
        Err(e) => {
            ffi_helpers::update_last_error(e);
            2
        }
    }
}

/// Free Rust-allocated memory.
#[no_mangle]
pub extern "C" fn sui_bcs_json_free(ptr: *const u8, len: usize) {
    unsafe {
        // SAFETY: not going to mutate the value via the pointer, so this is fine
        drop(Vec::from_raw_parts(ptr as *mut u8, len, len));
    }
}

/// Type is the value that we implement `DeserializeSeed` on. It stores the type being serialized,
/// just as a string -- there is no need to convert it into a tree-like structure.
#[derive(Copy, Clone)]
pub struct Type<'t>(&'t str);

/// Value is the intermediate representation between BCS and JSON. When going in either direction,
/// we go via an instance of `Value`.
pub enum Value {
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    U128(u128),
    // U256(U256),
    Bool(bool),
    Address(Address),
    InputArgument(InputArgument),
    MultiSig(MultiSig),
    MultiSigPublicKey(MultiSigPublicKey),
    String(String),
    Transaction(Transaction),
    TransactionKind(TransactionKind),
    Vec(Vec<Value>),
}

struct VectorVisitor<'t>(Type<'t>);

pub fn bcs_to_json(bytes: &[u8], type_: &str, pretty: bool) -> Result<String> {
    let value = bcs::from_bytes_seed(Type(type_), bytes)?;
    Ok(if pretty {
        serde_json::to_string_pretty(&value)?
    } else {
        serde_json::to_string(&value)?
    })
}

pub fn json_to_bcs(json: &str, type_: &str) -> Result<Vec<u8>> {
    let mut deserializer = serde_json::Deserializer::from_str(json);
    let value = Type(type_).deserialize(&mut deserializer)?;
    Ok(bcs::to_bytes(&value)?)
}

impl<'t> Type<'t> {
    fn is_vector(&self) -> Option<Type> {
        Some(Type(self.0.strip_prefix("vector<")?.strip_suffix(">")?))
    }
}

/// We need to implement a custom `Serialize` so that `Value` "ignores" its own outer enum. If we
/// used the derived implementation of `Serialize` for `Value`, then its BCS serialization would
/// include an index for the enum variant.
impl Serialize for Value {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use Value as V;
        match self {
            V::U8(n) => n.serialize(serializer),
            V::U16(n) => n.serialize(serializer),
            V::U32(n) => n.serialize(serializer),
            V::U64(n) => n.serialize(serializer),
            V::U128(n) => n.serialize(serializer),
            // V::U256(n) => n.serialize(serializer),
            V::Bool(b) => b.serialize(serializer),
            V::Address(a) => a.serialize(serializer),
            V::InputArgument(arg) => arg.serialize(serializer),
            V::MultiSig(sig) => sig.serialize(serializer),
            V::MultiSigPublicKey(sig) => sig.serialize(serializer),
            V::String(s) => serializer.serialize_str(s.as_str()),
            V::Transaction(data) => data.serialize(serializer),
            V::TransactionKind(kind) => kind.serialize(serializer),
            V::Vec(vs) => {
                let mut seq = serializer.serialize_seq(Some(vs.len()))?;
                for v in vs {
                    seq.serialize_element(v)?;
                }
                seq.end()
            }
        }
    }
}

impl<'t, 'de> DeserializeSeed<'de> for Type<'t> {
    type Value = Value;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        if let Some(inner) = self.is_vector() {
            return deserializer.deserialize_seq(VectorVisitor(inner));
        }

        struct Supported;
        impl Expected for Supported {
            fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                write!(
                    formatter,
                    "one of the following types: bool, u8, u16, u32, u64, u128, u256, address, \
                     call_arg, multisig, multisig_public_key, string, transaction_data, \
                     transaction_kind, or a vector of a supported type",
                )
            }
        }

        use Value as V;
        match self.0 {
            "bool" => bool::deserialize(deserializer).map(V::Bool),
            "u8" => u8::deserialize(deserializer).map(V::U8),
            "u16" => u16::deserialize(deserializer).map(V::U16),
            "u32" => u32::deserialize(deserializer).map(V::U32),
            "u64" => u64::deserialize(deserializer).map(V::U64),
            "u128" => u128::deserialize(deserializer).map(V::U128),
            // "u256" => U256::deserialize(deserializer).map(V::U256),
            "address" => Address::deserialize(deserializer).map(V::Address),
            "input_arg" => InputArgument::deserialize(deserializer).map(V::InputArgument),
            "multisig" => MultiSig::deserialize(deserializer).map(V::MultiSig),
            "multisig_public_key" => {
                MultiSigPublicKey::deserialize(deserializer).map(V::MultiSigPublicKey)
            }
            "transaction" => Transaction::deserialize(deserializer).map(V::Transaction),
            "transaction_kind" => {
                TransactionKind::deserialize(deserializer).map(V::TransactionKind)
            }
            "string" => String::deserialize(deserializer).map(V::String),
            unsupported => Err(D::Error::invalid_type(
                Unexpected::Other(unsupported),
                &Supported,
            )),
        }
    }
}

impl<'t, 'de> Visitor<'de> for VectorVisitor<'t> {
    type Value = Value;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(formatter, "Vector of values")
    }

    fn visit_seq<S: SeqAccess<'de>>(self, mut seq: S) -> Result<Self::Value, S::Error> {
        let mut vals = vec![];
        while let Some(elem) = seq.next_element_seed(self.0)? {
            vals.push(elem);
        }

        Ok(Value::Vec(vals))
    }
}
//**************************************************************************************************
// Error helper functions
//**************************************************************************************************

/// Get the length of the last error message in bytes when encoded as UTF-8,
/// including the trailing null. This function wraps last_error_length from ffi_helpers crate.
#[no_mangle]
pub extern "C" fn sui_last_error_length() -> c_int {
    ffi_helpers::error_handling::last_error_length()
}

/// Peek at the most recent error and write its error message (Display impl) into the provided
/// buffer as a UTF-8 encoded string.
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
    use std::str::FromStr;

    use sui_sdk2::types::{
        ConsensusCommitPrologue, Ed25519Signature, GasPayment, MultisigMemberSignature,
        TransactionExpiration,
    };

    use super::*;

    #[derive(Debug)]
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
        let bcs_from_json = json_to_bcs(&expected_json_str, typename).unwrap();
        let json_from_bcs = bcs_to_json(bcs_from_json.as_slice(), typename, true).unwrap();
        Data {
            expected_bcs,
            expected_json_str,
            bcs_from_json,
            json_from_bcs,
        }
    }

    #[test]
    fn test_vector() {
        let data = &vec![vec![10u8, 1u8, 127u8], vec![68u8]];
        let output = helper("vector<vector<u8>>", data);
        assert_eq!(output.expected_bcs, output.bcs_from_json);
        assert_eq!(output.expected_json_str, output.json_from_bcs);

        let data = &vec![1u8, 2u8, 3u8, 4u8];
        let output = helper("vector<u8>", data);
        assert_eq!(output.expected_bcs, output.bcs_from_json);
        assert_eq!(output.expected_json_str, output.json_from_bcs);

        let data = &vec![1u16, 2u16, 3u16, 4u16];
        let output = helper("vector<u16>", data);
        assert_eq!(output.expected_bcs, output.bcs_from_json);
        assert_eq!(output.expected_json_str, output.json_from_bcs);

        let data = &vec![10, 12, 30, 40];
        let output = helper("vector<u32>", data);
        assert_eq!(output.expected_bcs, output.bcs_from_json);
        assert_eq!(output.expected_json_str, output.json_from_bcs);

        let data = &vec![10214124u64, 12251251u64];
        let output = helper("vector<u64>", data);
        let expected = r#"[10214124, 12251251]"#;
        let json_obj: serde_json::Value = serde_json::from_str(expected).unwrap();
        let expected_json_str = serde_json::to_string_pretty(&json_obj).unwrap();
        assert_eq!(output.expected_bcs, output.bcs_from_json);
        assert_eq!(expected_json_str, output.json_from_bcs);

        let data = &vec![340_282_366_920_938_463_463_374_607u128];
        let output = helper("vector<u128>", data);
        let expected = r#"[340282366920938463463374607]"#;
        let json_obj: serde_json::Value = serde_json::from_str(expected).unwrap();
        let expected_json_str = serde_json::to_string_pretty(&json_obj).unwrap();
        assert_eq!(output.expected_bcs, output.bcs_from_json);
        assert_eq!(expected_json_str, output.json_from_bcs);

        // let number_str = "12880124512523626212541252364367345733";
        // let number_one = U256::from_str_radix(number_str, 10).unwrap();
        // let data = &vec![number_one];
        // let output = helper("vector<u256>", data);
        // let expected = r#"["12880124512523626212541252364367345733"]"#;
        // let json_obj: serde_json::Value = serde_json::from_str(expected).unwrap();
        // let expected_json_str = serde_json::to_string_pretty(&json_obj).unwrap();
        // assert_eq!(output.expected_bcs, output.bcs_from_json);
        // assert_eq!(expected_json_str, output.json_from_bcs);
        //
        // let data = &vec![vec![number_one]];
        // let output = helper("vector<vector<u256>>", data);
        // let expected = r#"[["12880124512523626212541252364367345733"]]"#;
        // let json_obj: serde_json::Value = serde_json::from_str(expected).unwrap();
        // let expected_json_str = serde_json::to_string_pretty(&json_obj).unwrap();
        // assert_eq!(output.expected_bcs, output.bcs_from_json);
        // assert_eq!(expected_json_str, output.json_from_bcs);
    }

    #[test]
    fn test_integer_from_json_string() {
        let data = "\"340282366920938463463374607\"";
        let output = helper("u128", data);
        let expected = 340282366920938463463374607u128;
        assert!(output.bcs_from_json == bcs::to_bytes(&expected).unwrap());
    }

    #[test]
    fn test_bool() {
        let data = true;
        let output = helper("bool", data);
        assert_eq!(output.expected_bcs, output.bcs_from_json);
        assert_eq!(output.expected_json_str, output.json_from_bcs);
        let data = false;
        let output = helper("bool", data);
        assert_eq!(output.expected_bcs, output.bcs_from_json);
        assert_eq!(output.expected_json_str, output.json_from_bcs);
    }

    #[test]
    fn test_u8() {
        let number = 16u8;
        let output = helper("u8", number);
        assert_eq!(output.expected_bcs, output.bcs_from_json);
        assert_eq!(output.expected_json_str, output.json_from_bcs);
    }

    #[test]
    fn test_u16() {
        let number = 161u16;
        let output = helper("u16", number);
        assert_eq!(output.expected_bcs, output.bcs_from_json);
        assert_eq!(output.expected_json_str, output.json_from_bcs);
    }

    #[test]
    fn test_u32() {
        let number = 255u32;
        let output = helper("u32", number);
        assert_eq!(output.expected_bcs, output.bcs_from_json);
        assert_eq!(output.expected_json_str, output.json_from_bcs);
    }

    #[test]
    fn test_u64() {
        let number = 12341u64;
        let output = helper("u64", number);
        assert_eq!(output.expected_bcs, output.bcs_from_json);
        assert_eq!(output.expected_json_str, output.json_from_bcs);
    }

    #[test]
    fn test_u128() {
        let number = 12341u128;
        let output = helper("u128", number);
        assert_eq!(output.expected_bcs, output.bcs_from_json);
        assert_eq!(output.expected_json_str, output.json_from_bcs);
    }

    // #[test]
    // fn test_u256() {
    //     let number_str = "12880124512523626212541252364367345733";
    //     let number = U256::from_str_radix(number_str, 10).unwrap();
    //     let output = helper("u256", number);
    //     assert_eq!(output.expected_bcs, output.bcs_from_json);
    //     assert_eq!(output.expected_json_str, output.json_from_bcs);
    // }

    #[test]
    fn test_string() {
        let input = "a";
        let output = helper("string", input);

        assert_eq!(output.expected_bcs, output.bcs_from_json);
        assert_eq!(output.expected_json_str, output.json_from_bcs);
    }

    #[test]
    fn test_sui_address() {
        let data =
            Address::from_str("0xf821d3483fc7725ebafaa5a3d12373d49901bdfce1484f219daa7066a30df77d")
                .unwrap();
        let output = helper("address", data);
        assert_eq!(output.expected_bcs, output.bcs_from_json);
        assert_eq!(output.expected_json_str, output.json_from_bcs);
    }

    #[test]
    fn test_consensus_commit_prologue() {
        let data = TransactionKind::ConsensusCommitPrologue(ConsensusCommitPrologue {
            epoch: 1,
            round: 1,
            commit_timestamp_ms: 215125,
        });

        let output = helper("transaction_kind", data.clone());
        assert_eq!(output.expected_bcs, output.bcs_from_json);
        assert_eq!(output.expected_json_str, output.json_from_bcs);

        // Why are these integers strings and not integers?
        let json = r#"{
            "kind":
            "consensus_commit_prologue",
            "epoch":"1",
            "round":"1",
            "commit_timestamp_ms":"215125"
        }"#;
        let bcs = json_to_bcs(json, "transaction_kind");
        assert!(bcs.is_ok());
    }

    #[test]
    fn test_tx_data_consensus_commit_prologue() {
        let kind = TransactionKind::ConsensusCommitPrologue(ConsensusCommitPrologue {
            epoch: 1,
            round: 1,
            commit_timestamp_ms: 215125,
        });
        let sender =
            Address::from_str("0xf821d3483fc7725ebafaa5a3d12373d49901bdfce1484f219daa7066a30df77d")
                .unwrap();
        let gas_payment = GasPayment {
            objects: vec![],
            owner: sender,
            price: 1000,
            budget: 5000,
        };
        let tx = Transaction {
            kind,
            sender,
            gas_payment,
            expiration: TransactionExpiration::None,
        };

        let output = helper("transaction", tx.clone());
        assert_eq!(output.expected_bcs, output.bcs_from_json);
        assert_eq!(output.expected_json_str, output.json_from_bcs);
    }

    #[test]
    fn test_multisig_and_multisig_public_key() {
        // these are copied from sui-types/src/unit_tests/multisig_tests.rs
        // for making multisignature objects.

        let mut rng = rand::thread_rng();
        let pk1 = Ed25519PublicKey::generate(rng.clone());
        let pk2 = Ed25519PublicKey::generate(rng.clone());
        let pk3 = Ed25519PublicKey::generate(rng);

        // let multisig_pk = MultiSigPublicKey::new(
        //     vec![pk1.clone(), pk2.clone(), pk3.clone()],
        //     vec![1, 1, 1],
        //     2,
        // )
        // .unwrap();
        //
        // let output = helper("multisig_public_key", multisig_pk.clone());
        // assert_eq!(output.expected_bcs, output.bcs_from_json);
        // assert_eq!(output.expected_json_str, output.json_from_bcs);
        //
        // let msg = IntentMessage::new(
        //     Intent::sui_transaction(),
        //     PersonalMessage {
        //         message: "Hello".as_bytes().to_vec(),
        //     },
        // );
        // let sig1: GenericSignature = Signature::new_secure(&msg, &keys[0]).into();
        // let sig2: GenericSignature = Signature::new_secure(&msg, &keys[1]).into();
        // let sig3: GenericSignature = Signature::new_secure(&msg, &keys[2]).into();
        //
        // // Any 2 of 3 signatures verifies ok. We are not interesting in veryfing the multisig, but
        // // only in encoding it to BCS and JSON, and decoding them.
        // let multi_sig1 =
        //     MultiSig::combine(vec![sig1.clone(), sig2.clone()], multisig_pk.clone()).unwrap();
        //
        // let output = helper("multisig", multi_sig1);
        // assert_eq!(output.expected_bcs, output.bcs_from_json);
        // assert_eq!(output.expected_json_str, output.json_from_bcs);
        //
        // let multi_sig2 =
        //     MultiSig::combine(vec![sig1.clone(), sig3.clone()], multisig_pk.clone()).unwrap();
        //
        // let output = helper("multisig", multi_sig2);
        // assert_eq!(output.expected_bcs, output.bcs_from_json);
        // assert_eq!(output.expected_json_str, output.json_from_bcs);
        //
        // let multi_sig3 =
        //     MultiSig::combine(vec![sig2.clone(), sig3.clone()], multisig_pk.clone()).unwrap();
        //
        // let output = helper("multisig", multi_sig3);
        // assert_eq!(output.expected_bcs, output.bcs_from_json);
        // assert_eq!(output.expected_json_str, output.json_from_bcs);
    }

    // #[test]
    // fn test_internal_bcs_to_json() {
    //     let mut ptb = ProgrammableTransactionBuilder::new();
    //     let split_coint_amount = ptb.pure(1000u64).unwrap(); // note that we need to specify the u64 type
    //     ptb.command(Command::SplitCoins(
    //         Argument::GasCoin,
    //         vec![split_coint_amount],
    //     ));
    //     let sender = SuiAddress::ZERO;
    //     let recipient = SuiAddress::ZERO;
    //     let argument_address = ptb.pure(recipient).unwrap();
    //     ptb.command(Command::TransferObjects(
    //         vec![Argument::Result(0)],
    //         argument_address,
    //     ));
    //
    //     let builder = ptb.finish();
    //     let gas_budget = 5_000_000;
    //     let gas_price = 1000;
    //
    //     let tx_data = TransactionData::new_programmable(
    //         sender,
    //         vec![random_object_ref()],
    //         builder,
    //         gas_budget,
    //         gas_price,
    //     );
    //     let output = helper("transaction_data", tx_data);
    //     assert_eq!(output.expected_bcs, output.bcs_from_json);
    //     assert_eq!(output.expected_json_str, output.json_from_bcs);
    // }

    // #[test]
    // fn test_ptb_to_internal_bcs_to_json() {
    //     let mut ptb = ProgrammableTransactionBuilder::new();
    //     let split_coint_amount = ptb.pure(1000u64).unwrap(); // note that we need to specify the u64 type
    //     ptb.command(Command::SplitCoins(SplitCoins {
    //         coin: Argument::GasCoin,
    //         amounts: vec![split_coint_amount],
    //     }));
    //     let sender = SuiAddress::ZERO;
    //     let recipient = SuiAddress::ZERO;
    //     let argument_address = ptb.pure(recipient).unwrap();
    //     ptb.command(Command::TransferObjects(
    //         vec![Argument::Result(0)],
    //         argument_address,
    //     ));
    //
    //     let builder = ptb.finish();
    //     let gas_budget = 5_000_000;
    //     let gas_price = 1000;
    //
    //     let tx_data = TransactionData::new_programmable(
    //         sender,
    //         vec![random_object_ref()],
    //         builder,
    //         gas_budget,
    //         gas_price,
    //     );
    //     let output = helper("transaction_data", tx_data);
    //     assert_eq!(output.expected_bcs, output.bcs_from_json);
    //     assert_eq!(output.expected_json_str, output.json_from_bcs);
    // }

    // #[test]
    // fn test_pay_sui_from_internal_json_to_bcs() {
    //     let amount = 1000u64;
    //     let sender = SuiAddress::ZERO;
    //     let validator = SuiAddress::ZERO;
    //     let gas_budget = 5_000_000;
    //     let gas_price = 1000;
    //
    //     let obj_vec = vec![ObjectArg::ImmOrOwnedObject(random_object_ref())];
    //     let pt = {
    //         let mut builder = ProgrammableTransactionBuilder::new();
    //         let arguments = vec![
    //             // builder.input(CallArg::SUI_SYSTEM_MUT).unwrap(),
    //             builder.make_obj_vec(obj_vec).unwrap(),
    //             builder
    //                 .input(CallArg::Pure(bcs::to_bytes(&amount).unwrap()))
    //                 .unwrap(),
    //             builder
    //                 .input(CallArg::Pure(bcs::to_bytes(&validator).unwrap()))
    //                 .unwrap(),
    //         ];
    //         builder.command(Command::move_call(
    //             SUI_SYSTEM_PACKAGE_ID,
    //             SUI_SYSTEM_MODULE_NAME.to_owned(),
    //             ADD_STAKE_MUL_COIN_FUN_NAME.to_owned(),
    //             vec![],
    //             arguments,
    //         ));
    //         builder.finish()
    //     };
    //     let tx_data = TransactionData::new_programmable(
    //         sender,
    //         vec![random_object_ref()],
    //         pt,
    //         gas_budget,
    //         gas_price,
    //     );
    //     let output = helper("transaction_data", tx_data);
    //     assert_eq!(output.expected_bcs, output.bcs_from_json);
    //     assert_eq!(output.expected_json_str, output.json_from_bcs);
    // }
}
