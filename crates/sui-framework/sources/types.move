// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Sui types helpers and utilities
module sui::types {
    use std::ascii;
    use std::string::{Self, String, sub_string};
    use std::type_name;

    /// Package of type `T` differs from `Witness` package
    const EWitnessSourcePackageMismatch: u64 = 0;

    /// Module of type `T` differs from `Witness` module
    const EWitnessSourceModuleMismatch: u64 = 0;

    public fun assert_same_module<T, Witness>() {
        let (package_a, module_a, _) = get_package_module_type<T>();
        let (package_b, module_b, _) = get_package_module_type<Witness>();

        assert!(package_a == package_b, EWitnessSourcePackageMismatch);
        assert!(module_a == module_b, EWitnessSourceModuleMismatch);
    }

    public fun get_package_module_type<T>(): (String, String, String) {
        let delimiter = string::utf8(b"::");

        let t = string::utf8(ascii::into_bytes(
            type_name::into_string(type_name::get<T>())
        ));

        // TBD: this can probably be hard-coded as all hex addrs are 32 bytes
        let package_delimiter_index = string::index_of(&t, &delimiter);
        let package_addr = sub_string(&t, 0, string::index_of(&t, &delimiter));

        let tail = sub_string(&t, package_delimiter_index + 2, string::length(&t));

        let module_delimiter_index = string::index_of(&tail, &delimiter);
        let module_name = sub_string(&tail, 0, module_delimiter_index);

        let type_name = sub_string(&tail, module_delimiter_index + 2, string::length(&tail));

        (package_addr, module_name, type_name)
    }

    // === one-time witness ===

    /// Tests if the argument type is a one-time witness, that is a type with only one instantiation
    /// across the entire code base.
    public native fun is_one_time_witness<T: drop>(_: &T): bool;

    spec is_one_time_witness {
        pragma opaque;
        // TODO: stub to be replaced by actual abort conditions if any
        aborts_if [abstract] true;
        // TODO: specify actual function behavior
    }
}
