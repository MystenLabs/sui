// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Utility module implementing the permissions for the `Kiosk`.
///
/// Permissions:
/// - ...00001 `place`
/// - ...00010 `lock`
module sui::kiosk_permissions {

    /// Check whether the first bit of the value is set (odd value)
    public fun can_place(permissions: u32): bool { permissions & 0x01 != 0 }
    /// Check whether the first bit of the value is set (odd value)
    public fun can_lock(permissions: u32): bool { permissions & 0x02 != 0 }

    /// Add the `place_as_extension` and `lock_as_extension` permission to the permissions set.
    public fun add_place(permissions: &mut u32) { *permissions = *permissions | 0x01 }
    /// Add the `borrow_as_extension` permission to the permissions set.
    public fun add_lock(permissions: &mut u32) { *permissions = *permissions | 0x02 }

    /// Get the maximum permissions value.
    public fun max_permissions(): u32 { 0x03 }

    #[test]
    /// Test the bits of the value.
    fun test_permissions() {
        assert!(check(0x0) == vector[false, false], 0); // 000
        assert!(check(0x1) == vector[false, true], 0);  // 001
        assert!(check(0x2) == vector[true, false], 0);  // 010
        assert!(check(0x3) == vector[true, true], 0);   // 011
    }

    #[test_only]
    /// Turn the bits into a vector of booleans for testing.
    fun check(self: u32): vector<bool> {
        vector[
            can_lock(self),
            can_place(self),
        ]
    }

    #[test]
    fun kiosk_permissions() {
        let permissions = 0u32;
        assert!(!can_place(permissions), 0);
        assert!(!can_lock(permissions), 1);

        add_place(&mut permissions);
        assert!(can_place(permissions), 3);
        assert!(!can_lock(permissions), 4);

        add_lock(&mut permissions);
        assert!(can_place(permissions), 6);
        assert!(can_lock(permissions), 7);
    }
}
