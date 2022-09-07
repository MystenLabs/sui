// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Library for Elliptic Curve operations on chain. We specifically support the Ristretto-255 sub-group.
module sui::elliptic_curve {
    use std::vector;
    use std::debug;

    ///////////////////////////////////
    /// Elliptic Curve structs
    ///////////////////////////////////

    /// Represents a point on the Ristretto-255 subgroup.
    struct RistrettoPoint has copy, drop, store {
        // A 32-byte representation of the group element.
        value: vector<u8>
    }

    /// Represents a scalar within the Curve25519 prime-order group.
    struct Scalar has copy, drop, store {
        // A 32-byte representation of the scalar
        value: vector<u8>
    }

    ///////////////////////////////////
    /// Private 
    ///////////////////////////////////

    /// @param value: The value to commit to
    /// @param blinding_factor: A random number used to ensure that the commitment is hiding.
    native fun native_create_pedersen_commitment(value: vector<u8>, blinding_factor: vector<u8>): vector<u8>;

    /// @param self: bytes representation of an EC point on the Ristretto-255 subgroup 
    /// @param other: bytes representation of an EC point on the Ristretto-255 subgroup 
    /// A native move wrapper around the addition of Ristretto points. Returns self + other.
    native fun native_add_ristretto_point(point1: vector<u8>, point2: vector<u8>): vector<u8>;

    /// @param self: bytes representation of an EC point on the Ristretto-255 subgroup
    /// @param other: bytes representation of an EC point on the Ristretto-255 subgroup 
    /// A native move wrapper around the subtraction of Ristretto points. Returns self - other.
    native fun native_subtract_ristretto_point(point1: vector<u8>, point2: vector<u8>): vector<u8>;

    /// @param value: the value of the to-be-created scalar
    /// TODO: Transfer this into a Move function some time in the future.
    /// A native move wrapper for the creation of Scalars on Curve25519.
    native fun native_scalar_from_u64(value: u64): vector<u8>;


    /// @param value: the bytes representation of the scalar.
    /// TODO: Transfer this into a Move function some time in the future.
    /// A native move wrapper for the creation of Scalars on Curve25519.
    native fun native_scalar_from_bytes(bytes: vector<u8>): vector<u8>;

    ///////////////////////////////////
    /// Public
    ///////////////////////////////////
    
    // Scalar
    ///////////////////////

    /// Create a field element from u64
    public fun new_scalar_from_u64(value: u64): Scalar {
        debug::print(&value);
        Scalar {
            value: native_scalar_from_u64(value)
        }
    }

    /// Create a pedersen commitment from two field elements
    public fun create_pedersen_commitment(value: Scalar, blinding_factor: Scalar): RistrettoPoint {
        return RistrettoPoint {
            value: native_create_pedersen_commitment(value.value, blinding_factor.value)
        }
    }

    /// Creates a new field element from byte representation. Note that
    /// `value` must be 32-bytes
    public fun new_scalar_from_bytes(value: vector<u8>): Scalar {
        Scalar {
            value: native_scalar_from_bytes(value)
        }
    }

    /// Get the byte representation of the field element
    public fun scalar_bytes(self: &Scalar): vector<u8> {
        self.value
    }
    
    // EC Point
    ///////////////////////

    /// Get the underlying compressed byte representation of the group element
    public fun bytes(self: &RistrettoPoint): vector<u8> {
        self.value
    }


    /// Perform addition on two group elements
    public fun add(self: &RistrettoPoint, other: &RistrettoPoint): RistrettoPoint {
        RistrettoPoint {
            value: native_add_ristretto_point(self.value, other.value)
        }
    }

    /// Perform subtraction on two group elements
    public fun subtract(self: &RistrettoPoint, other: &RistrettoPoint): RistrettoPoint {
        RistrettoPoint {
            value: native_subtract_ristretto_point(self.value, other.value)
        }
    }

    /// Attempt to create a new group element from compressed bytes representation
    public fun new_from_bytes(bytes: vector<u8>): RistrettoPoint {
        assert!(vector::length(&bytes) == 32, 1);
        RistrettoPoint {
            value: bytes
        }
    }

    // TODO: Add arithmetic for Scalar elements. We just need add, subtract, and multiply.
    // TODO: Add scalar to point multiplication for group elements.
}
