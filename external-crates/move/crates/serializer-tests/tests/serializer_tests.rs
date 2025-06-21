// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::{binary_config::BinaryConfig, file_format::CompiledModule};
use proptest::prelude::*;

proptest! {
    #[test]
    fn serializer_roundtrip(module in CompiledModule::valid_strategy(30)) {
        let mut serialized = vec![];
        module.serialize(&mut serialized).expect("serialization should work");

        // Metadata may be generated as part of the proptest, so we need to allow extra bytes.
        let config = BinaryConfig::with_extraneous_bytes_check(false);

        let deserialized_module = CompiledModule::deserialize_with_config(&serialized, &config)
            .expect("deserialization should work");

        prop_assert_eq!(module, deserialized_module);
    }

    #[test]
    fn serializer_roundtrip_with_defaults(mut module in CompiledModule::valid_strategy(30)) {
        // Metadata may be generated as part of the proptest, so we need to allow extra bytes.
        module.metadata = vec![];
        let mut serialized = vec![];
        module.serialize(&mut serialized).expect("serialization should work");

        let deserialized_module = CompiledModule::deserialize_with_defaults(&serialized)
            .expect("deserialization should work");

        prop_assert_eq!(module, deserialized_module);
    }

}

proptest! {
    // Generating arbitrary compiled modules is really slow, possibly because of
    // https://github.com/AltSysrq/proptest/issues/143.
    #![proptest_config(ProptestConfig::with_cases(16))]

    /// Make sure that garbage inputs don't crash the serializer and deserializer.
    #[test]
    fn garbage_inputs(module in any_with::<CompiledModule>(16)) {
        let mut serialized = Vec::with_capacity(65536);
        module.serialize(&mut serialized).expect("serialization should work");

        let deserialized_module = CompiledModule::deserialize_no_check_bounds(&serialized)
            .expect("deserialization should work");
        prop_assert_eq!(module, deserialized_module);
    }
}
