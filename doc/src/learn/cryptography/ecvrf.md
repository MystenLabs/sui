---
title: Verifiable Random Function (VRF)
---

A Verifiable Random Function (VRF) is a cryptographic primitive that allows a party to generate a random number and prove to another party that the number was generated using a secret key. 
The proof can be verified by anyone using the public key corresponding to the secret key, so it can be used as a random number generator (RNG) which generates outputs that are verifiable by anyone, and may be used 
by applications that need verifiable randomness on-chain.

## VRF construction
The VRF used in the Move API in Sui is an Elliptic Curve VRF (ECVRF) following the [CFRG VRF draft specifications version 15](https://datatracker.ietf.org/doc/draft-irtf-cfrg-vrf/15/). It uses [Ristretto255](https://ristretto.group) elliptic curve group construction with the SHA-512 hash function. The nonce is generated according to [RFC6979](https://www.rfc-editor.org/info/rfc6979).

Any implementation following the same specifications with suite string `sui_vrf` (see section 5 in the [VRF specs](https://datatracker.ietf.org/doc/draft-irtf-cfrg-vrf/15/)) can be used to compute VRF output and generate proofs.

The [fastcrypto](https://github.com/MystenLabs/fastcrypto) library provides a CLI tool for such an implementation and will be used in the example below.

### Generate keys
From the root of the `fastcrypto` repository, run the following command to generate a key pair:
```shell
cargo run --bin ecvrf-cli keygen
```
This outputs a secret key and a public key in hex format. The secret key is a 32-byte string and the public key is a 32-byte string:
````shell
Secret key: c0cbc5bf0b2f992fe14fee0327463c7b03d14cbbcb38ce2584d95ee0c112b40b
Public key: 928744da5ffa614d65dd1d5659a8e9dd558e68f8565946ef3d54215d90cba015
````

### Compute VRF output and proof
To compute the VRF output and proof for the input string `Hello, World!`, which is `48656c6c6f2c20776f726c6421` in hexadecimal, with the key pair generated above we run the following command:
```shell
cargo run --bin ecvrf-cli prove --input 48656c6c6f2c20776f726c6421 --secret-key c0cbc5bf0b2f992fe14fee0327463c7b03d14cbbcb38ce2584d95ee0c112b40b
```
This should the 80 byte proof and VRF 64 byte output, both in hex format:
```shell
Proof:  18ccf8bf316f00b387fc6e7b26f2d3ddadbf5e9c66d3a30986f12b208108551f9c6da87793a857d79261338a50430074b1dbc7f8f05e492149c51313381248b4229ebdda367146dbbbf95809c7fb330d
Output: 2b7e45821d80567761e8bb3fc519efe5ad80cdb4423227289f960319bbcf6eea1aef30c023617d73f589f98272b87563c6669f82b51dafbeb5b9cf3b17c73437
```

### Verify proof
The proof and output can be verified in a smart contract using `sui::ecvrf::ecvrf_verify` from the Sui Move framework:
```move
module math::ecvrf_test {
    use sui::ecvrf;
    use sui::event;

    /// Event on whether the output is verified
    struct VerifiedEvent has copy, drop {
        is_verified: bool,
    }

    public entry fun verify_ecvrf_output(output: vector<u8>, alpha_string: vector<u8>, public_key: vector<u8>, proof: vector<u8>) {
        event::emit(VerifiedEvent {is_verified: ecvrf::ecvrf_verify(&output, &alpha_string, &public_key, &proof)});
    }
}
```
It can also be verfied using the CLI tool:
```shell
cargo run --bin ecvrf-cli verify --output 2b7e45821d80567761e8bb3fc519efe5ad80cdb4423227289f960319bbcf6eea1aef30c023617d73f589f98272b87563c6669f82b51dafbeb5b9cf3b17c73437 --proof 18ccf8bf316f00b387fc6e7b26f2d3ddadbf5e9c66d3a30986f12b208108551f9c6da87793a857d79261338a50430074b1dbc7f8f05e492149c51313381248b4229ebdda367146dbbbf95809c7fb330d --input 48656c6c6f2c20776f726c6421 --public-key 928744da5ffa614d65dd1d5659a8e9dd558e68f8565946ef3d54215d90cba015
```
which outputs
```shell
Proof verified correctly!
```
