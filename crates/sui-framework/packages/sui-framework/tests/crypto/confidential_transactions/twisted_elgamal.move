module sui::twisted_elgamal;

use sui::group_ops::Element;
use sui::ristretto255::{Self, Point};

public fun g(): Element<Point> {
    ristretto255::generator()
}

public fun h(): Element<Point> {
    ristretto255::point_from_bytes(
        &x"34ce1477c14558178089500a39c864e0f607b3c1f41ab398400e4a9de6d2c446",
    )
}

public struct Encryption has copy, drop, store {
    ciphertext: Element<Point>,
    decryption_handle: Element<Point>,
}

public fun ciphertext(e: &Encryption): &Element<Point> {
    &e.ciphertext
}

public fun decryption_handle(e: &Encryption): &Element<Point> {
    &e.decryption_handle
}

public(package) fun new(ciphertext: Element<Point>, decryption_handle: Element<Point>): Encryption {
    Encryption {
        ciphertext,
        decryption_handle,
    }
}

/// Create a new Twisted ElGamal encryption:
///  - ciphertext = r*G + m*H
///  - decryption_handle = r*pk = r*x*G
public fun encryption(ciphertext: Element<Point>, decryption_handle: Element<Point>): Encryption {
    Encryption {
        ciphertext,
        decryption_handle,
    }
}

/// Trivial encryption without randomness.
public fun encrypt_zero(pk: &Element<Point>): Encryption {
    Encryption {
        ciphertext: g(),
        decryption_handle: *pk,
    }
}

/// Trivial encryption without randomness.
public fun encrypt_trivial(amount: u16, pk: &Element<Point>): Encryption {
    Encryption {
        ciphertext: ristretto255::point_add(
            &g(),
            &ristretto255::point_mul(&ristretto255::scalar_from_u64(amount as u64), &h()),
        ),
        decryption_handle: *pk,
    }
}

/// Add two Twisted ElGamal encryptions. The result is an encryption of the sum of the plaintexts in the scalar field, so beware of overflow.
fun add(e1: &Encryption, e2: &Encryption): Encryption {
    Encryption {
        ciphertext: ristretto255::point_add(&e1.ciphertext, &e2.ciphertext),
        decryption_handle: ristretto255::point_add(&e1.decryption_handle, &e2.decryption_handle),
    }
}

fun sub(e1: &Encryption, e2: &Encryption): Encryption {
    Encryption {
        ciphertext: ristretto255::point_sub(&e1.ciphertext, &e2.ciphertext),
        decryption_handle: ristretto255::point_sub(&e1.decryption_handle, &e2.decryption_handle),
    }
}

fun shift_left(e: &Encryption, bits: u8): Encryption {
    let factor = ristretto255::scalar_from_u64(1 << bits);
    Encryption {
        ciphertext: ristretto255::point_mul(&factor, &e.ciphertext),
        decryption_handle: ristretto255::point_mul(&factor, &e.decryption_handle),
    }
}

public fun add_assign(a: &mut EncryptedAmount4U32, b: &EncryptedAmount4U16) {
    a.l0 = add(&a.l0, &b.l0);
    a.l1 = add(&a.l1, &b.l1);
    a.l2 = add(&a.l2, &b.l2);
    a.l3 = add(&a.l3, &b.l3);
}

fun encrypted_amount_2_u32_unverified_to_encryption(
    self: &EncryptedAmount2U32Unverified,
): Encryption {
    self.l0.add(&self.l1.shift_left(32))
}

// Encrypted u64 amount.
// Stored as two u32 encryptions, which can be decrypted by user.
// Value is l0 + 2^32 * l1.
// Well formedness is not verified.
public struct EncryptedAmount2U32Unverified has copy, drop, store {
    // maybe phantom T?
    l0: Encryption,
    l1: Encryption,
}

public fun encrypted_amount_2_u32_unverified(
    encryptions: vector<Encryption>,
): EncryptedAmount2U32Unverified {
    assert!(encryptions.length() == 2);
    EncryptedAmount2U32Unverified {
        l0: encryptions[0],
        l1: encryptions[1],
    }
}

public fun encrypted_amount_2_u_32_zero(pk: &Element<Point>): EncryptedAmount2U32Unverified {
    EncryptedAmount2U32Unverified {
        l0: encrypt_zero(pk),
        l1: encrypt_zero(pk),
    }
}

public(package) fun encrypted_amount_2_u_32_zero_verify_non_negative(
    ea: &EncryptedAmount2U32Unverified,
    proof: &vector<u8>,
): bool {
    // TODO: No need to also add decryption handles here
    let value = ea.l0.add(&ea.l1.shift_left(32));
    std::debug::print(&value);
    ristretto255::verify_range_proof(proof, 64, &vector[value.ciphertext])
}

// Encrypted u64 amount.
// Stored as four u16 encryptions, which can be decrypted by user and aggregated.
// Value is l0 + 2^16 * l1 + 2^32 * l2 + 2^48 * l3.
// Well formedness is verified.
public struct EncryptedAmount4U16 has copy, drop, store {
    // maybe phantom T?
    l0: Encryption,
    l1: Encryption,
    l2: Encryption,
    l3: Encryption,
}

public fun encrypted_amount_4_u16(encryptions: vector<Encryption>): EncryptedAmount4U16 {
    assert!(encryptions.length() == 4);
    EncryptedAmount4U16 {
        l0: encryptions[0],
        l1: encryptions[1],
        l2: encryptions[2],
        l3: encryptions[3],
    }
}

public fun encrypted_amount_4_u16_from_value(value: u64, pk: &Element<Point>): EncryptedAmount4U16 {
    EncryptedAmount4U16 {
        l0: encrypt_trivial((value & 0xFFFF) as u16, pk),
        l1: encrypt_trivial(((value >> 16) & 0xFFFF) as u16, pk),
        l2: encrypt_trivial(((value >> 32) & 0xFFFF) as u16, pk),
        l3: encrypt_trivial(((value >> 48) & 0xFFFF) as u16, pk),
    }
}

public(package) fun encrypted_amount_4_u16_to_encryption(ea: &EncryptedAmount4U16): Encryption {
    ea
        .l0
        .add(
            &ea.l1.shift_left(16).add(&ea.l2.shift_left(32)).add(&ea.l3.shift_left(48)),
        )
}

public(package) fun encrypted_amount_4_u16_verify_range(
    ea: &EncryptedAmount4U16,
    proof: &vector<u8>,
): bool {
    let commitments = vector[
        ea.l0.ciphertext,
        ea.l1.ciphertext,
        ea.l2.ciphertext,
        ea.l3.ciphertext,
    ];
    ristretto255::verify_range_proof(proof, 16, &commitments)
}

/// Verify a NIZK proof that the encrypted amount equals the given value.
public fun verify_value_proof(
    amount: u64,
    ea: &EncryptedAmount4U16,
    pk: &Element<Point>,
    proof: vector<u8>,
): bool {
    let proof = sui::nizk::from_bytes(proof);
    let encryption = encrypted_amount_4_u16_to_encryption(ea);
    proof.verify(
        &b"",
        &ristretto255::generator(),
        &ristretto255::point_sub(
            &encryption.ciphertext,
            &ristretto255::point_mul(&ristretto255::scalar_from_u64(amount), &h()),
        ),
        pk,
        &encryption.decryption_handle,
    )
}

/// Verify a NIZK proof that the encrypted amount equals the given value where the encryption is created by someone who knows the blinding factor.
public fun verify_value_proof_to_other(
    amount: u64,
    ea: &EncryptedAmount4U16,
    pk: &Element<Point>,
    proof: vector<u8>,
): bool {
    let proof = sui::nizk::from_bytes(proof);
    let encryption = encrypted_amount_4_u16_to_encryption(ea);
    proof.verify(
        &b"",
        &ristretto255::generator(),
        pk,
        &ristretto255::point_sub(
            &encryption.ciphertext,
            &ristretto255::point_mul(&ristretto255::scalar_from_u64(amount), &h()),
        ),
        &encryption.decryption_handle,
    )
}

/// Verify a NIZK proof that sum = a + b
public fun verify_sum_proof(
    sum: &EncryptedAmount2U32Unverified,
    a: &EncryptedAmount2U32Unverified,
    b: &EncryptedAmount4U32,
    pk: &Element<Point>,
    proof: vector<u8>,
): bool {
    verify_sum_proof_with_encryption(sum, a, &encrypted_amount_4_u32_to_encryption(b), pk, proof)
}

/// Verify a NIZK proof that sum = a + b
public fun verify_sum_proof_with_encryption(
    sum: &EncryptedAmount2U32Unverified,
    a: &EncryptedAmount2U32Unverified,
    b: &Encryption,
    pk: &Element<Point>,
    proof: vector<u8>,
): bool {
    let proof = sui::nizk::from_bytes(proof);
    let sum_encryption = encrypted_amount_2_u32_unverified_to_encryption(sum);
    let a_encryption = encrypted_amount_2_u32_unverified_to_encryption(a);
    let zero_encryption = sub(&add(&a_encryption, b), &sum_encryption);
    proof.verify(
        &b"",
        &ristretto255::generator(),
        &zero_encryption.ciphertext,
        pk,
        &zero_encryption.decryption_handle,
    )
}

/// Verify a NIZK proof that there is r such that d1 = r*p1 and d2 = r*p2
public fun verify_handle_eq(
    p1: &Element<Point>,
    p2: &Element<Point>,
    d1: &Element<Point>,
    d2: &Element<Point>,
    proof: vector<u8>,
): bool {
    sui::nizk::from_bytes(proof).verify(
        &b"",
        p1,
        p2,
        d1,
        d2,
    )
}

#[test]
fun test_value_proof() {
    // Test vector from fastcrypto
    let sk = ristretto255::scalar_from_bytes(
        &x"ee6b6b93ae724ae3aafee361c94ea83c3b0d29f86de5e94b6e648d79ab0fa705",
    );
    let pk = ristretto255::point_mul(&sk, &ristretto255::generator());

    let amount = 1234u32;
    let amount_as_scalar = ristretto255::scalar_from_u64(amount as u64);
    let c = ristretto255::point_from_bytes(
        &x"128ede6d07554b171b0964a351fce7b925a47356bac064de626a582c1b7df559",
    );
    let d = ristretto255::point_from_bytes(
        &x"28f6b0c7d009fe315cd24e3b6331514704bbdd1a0fbf4d25828061f40b401174",
    );
    let a = ristretto255::point_from_bytes(
        &x"e89da20d4e3369ef83a70dc1ca145f1bc0868e4b641656f08327bc0ca4ebfa19",
    );
    let b = ristretto255::point_from_bytes(
        &x"2ef862bb8e0a11e7912168dbce9513a756bc960f518a8fc1da7c8429a84ad067",
    );
    let z = ristretto255::scalar_from_bytes(
        &x"dc62c53a7b7c297f1849ce43d34a617722d5222397df3dcb9fcbb878ae8a9204",
    );

    let proof = sui::nizk::new(a, b, z);

    assert!(
        proof.verify(
            &x"",
            &ristretto255::generator(),
            &ristretto255::point_sub(&c, &ristretto255::point_mul(&amount_as_scalar, &h())),
            &pk,
            &d,
        ),
    );
}

// Encrypted u64 amount.
// Four u16 encryptions that may overflow to u32.
// Value is l0 + 2^16 * l1 + 2^32 * l2 + 2^48 * l3.
// Well formedness is verified.
public struct EncryptedAmount4U32 has copy, drop, store {
    l0: Encryption,
    l1: Encryption,
    l2: Encryption,
    l3: Encryption,
}

public fun encrypted_amount_4_u32(encryptions: vector<Encryption>): EncryptedAmount4U32 {
    assert!(encryptions.length() == 4);
    EncryptedAmount4U32 {
        l0: encryptions[0],
        l1: encryptions[1],
        l2: encryptions[2],
        l3: encryptions[3],
    }
}

public fun encrypted_amount_4_u32_zero(pk: &Element<Point>): EncryptedAmount4U32 {
    EncryptedAmount4U32 {
        l0: encrypt_zero(pk),
        l1: encrypt_zero(pk),
        l2: encrypt_zero(pk),
        l3: encrypt_zero(pk),
    }
}

public fun encrypted_amount_4_u32_from_4_u16(ea: EncryptedAmount4U16): EncryptedAmount4U32 {
    let EncryptedAmount4U16 {
        l0,
        l1,
        l2,
        l3,
    } = ea;
    EncryptedAmount4U32 {
        l0,
        l1,
        l2,
        l3,
    }
}

public fun encrypted_amount_4_u32_to_encryption(eq: &EncryptedAmount4U32): Encryption {
    eq.l0.add(&eq.l1.shift_left(16).add(&eq.l2.shift_left(32)).add(&eq.l3.shift_left(48)))
}
