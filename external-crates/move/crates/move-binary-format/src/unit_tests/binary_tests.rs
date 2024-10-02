// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{file_format::Bytecode, file_format_common::*};
use proptest::prelude::*;

#[test]
fn size_canary() {
    use crate::file_format::SignatureToken;
    use std::mem::size_of;
    assert_eq!(size_of::<SignatureToken>(), 16);
    assert_eq!(size_of::<Bytecode>(), 16);
}

#[test]
fn binary_len() {
    let mut binary_data = BinaryData::new();
    for _ in 0..100 {
        binary_data.push(1).unwrap();
    }
    assert_eq!(binary_data.len(), 100);
}

#[test]
fn test_max_number_of_bytecode() {
    let mut nops = vec![];
    for _ in 0..u16::MAX - 1 {
        nops.push(Bytecode::Nop);
    }
    nops.push(Bytecode::Branch(0));

    let result = Bytecode::get_successors(u16::MAX - 1, &nops, &[]);
    assert_eq!(result, vec![0]);
}

proptest! {
    #[test]
    fn vec_to_binary(vec in any::<Vec<u8>>()) {
        let binary_data = BinaryData::from(vec.clone());
        let vec2 = binary_data.into_inner();
        assert_eq!(vec.len(), vec2.len());
    }
}

proptest! {
    #[test]
    fn binary_push(item in any::<u8>()) {
        let mut binary_data = BinaryData::new();
        binary_data.push(item).unwrap();
        assert_eq!(binary_data.into_inner()[0], item);
    }
}

proptest! {
    #[test]
    fn binary_extend(vec in any::<Vec<u8>>()) {
        let mut binary_data = BinaryData::new();
        binary_data.extend(&vec).unwrap();
        assert_eq!(binary_data.len(), vec.len());
        for (index, item) in vec.iter().enumerate() {
            assert_eq!(*item, binary_data.as_inner()[index]);
        }
    }
}

#[test]
fn test_flavor() {
    for i in 1..=6 {
        let encoded = BinaryFlavor::encode_version(i);
        assert_eq!(encoded, i);
        assert_eq!(BinaryFlavor::decode_version(encoded), i);
        assert_eq!(BinaryFlavor::decode_flavor(encoded), None);
    }

    for i in 7..1024 {
        let flavored = BinaryFlavor::encode_version(i);
        assert_eq!(BinaryFlavor::decode_version(flavored), i);
        assert_eq!(
            BinaryFlavor::decode_flavor(flavored),
            Some(BinaryFlavor::SUI_FLAVOR)
        );
    }
}
