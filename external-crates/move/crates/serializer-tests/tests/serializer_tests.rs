// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::CompiledModule;
use proptest::prelude::*;

proptest! {
    #[test]
    fn serializer_roundtrip(module in CompiledModule::valid_strategy(20)) {
        let mut serialized = Vec::with_capacity(2048);
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

proptest! {
    #[test]
    fn identifier_serializer_roundtrip(module in CompiledModule::valid_strategy(20)) {
        let module_id = module.self_id();
        let deserialized_module_id = {
            let serialized_key = bcs::to_bytes(&module_id).unwrap();
            bcs::from_bytes(&serialized_key).expect("Deserialize should work")
        };
        prop_assert_eq!(module_id, deserialized_module_id);
    }
}
