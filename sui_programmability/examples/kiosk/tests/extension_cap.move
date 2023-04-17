/// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Kiosk Extension Capabilities
/// [Lock, List, Borrow, Place]
///
/// 0000 - for bits - one for each of the actions
/// 1001 - can place
/// 1111 - can do everything
/// 1100 - can borrow
///
/// Extension Capability does not allow: `take` and `list`. Any third party can
/// expose access to the witness data of the extension therefore creating a security
/// vulnerability of one's Kiosk.
module kiosk::extension_cap {

    /// Holds the bitmask for the set of permissions of the extension.
    struct ExtensionCap has store, drop { value: u16 }

    /// Check whether the first bit of the value is set (odd value)
    public fun can_place(self: &ExtensionCap): bool { self.value & 0x01 != 0 }
    /// Check whether the second bit of the value is set (value greater than 2)
    public fun can_borrow(self: &ExtensionCap): bool { self.value & 0x02 != 0 }
    /// Check whether the fifth bit of the value is set (value greater than 16)
    public fun can_lock(self: &ExtensionCap): bool { self.value & 0x10 != 0 }

    #[test]
    /// Test the bits of the value.
    fun test_bits() {
        assert!(check(ExtensionCap { value: 0x0 }) == vector[false, false, false, false, false], 0);
        assert!(check(ExtensionCap { value: 0x1 }) == vector[false, false, false, false, true], 0);
        assert!(check(ExtensionCap { value: 0x2 }) == vector[false, false, false, true, false], 0);
        assert!(check(ExtensionCap { value: 0x3 }) == vector[false, false, false, true, true], 0);
        assert!(check(ExtensionCap { value: 0x4 }) == vector[false, false, true, false, false], 0);
        assert!(check(ExtensionCap { value: 0x5 }) == vector[false, false, true, false, true], 0);
        assert!(check(ExtensionCap { value: 0x8 }) == vector[false, true, false, false, false], 0);
        assert!(check(ExtensionCap { value: 0x9 }) == vector[false, true, false, false, true], 0);
        assert!(check(ExtensionCap { value: 0x11 }) == vector[true, false, false, false, true], 0);
    }

    #[test_only]
    /// Turn the bits into a vector of booleans for testing.
    fun check(self: ExtensionCap): vector<bool> {
        vector[
            can_lock(&self),
            can_list(&self),
            can_take(&self),
            can_borrow(&self),
            can_place(&self),
        ]
    }
}
