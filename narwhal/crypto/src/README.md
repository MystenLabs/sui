# Signature Schemes and Traits

This crate contains:
- useful traits that should be implemented by concrete types that represent digital cryptographic 
material (keys). For signatures, we rely on `signature::Signature`, which may be more widely implemented.
- concrete implementations of the following signature schemes that implement the recommended traits required for 
cryptographic agility. The following schemes are implemented (wrappers over existing popular crates):
    - ed25519 (EdDSA), backed by the [dalek-ed25519](https://github.com/dalek-cryptography/ed25519-dalek) crate (soon 
to be replaced by the [ed25519-consensus](https://github.com/penumbra-zone/ed25519-consensus) crate).
    - Secp256k1, backed by the [k256](https://docs.rs/k256/latest/k256/) crate. 
    - BLS12-381, backed by the [blst](https://github.com/supranational/blst) crate.
    - BLS12-377, backed by the [ark_bls12_377](https://docs.rs/ark-bls12-377/0.3.0/ark_bls12_377/) crate. *Note that this
implementation is under the non-default conditional compilation feature `celo`, and is not compiled or linked without 
that explicit flag being passed in. The goal of this implementation is to provide an experimental benchmark, it is NOT 
meant as a production implementation.*
- An asynchronous [`SignatureService`] (which lives in `lib.rs`) that is instantiated by a `Signer` object.

## Traits
- [`ToFromBytes`]: this trait aims to minimize the number of steps involved in obtaining a serializable key.
- [`EncodeDecodeBase64`]: an extension trait of `ToFromBytes` for immediate conversion to/from base64 strings.
- [`VerifyingKey`]: which denotes associated types for private key and signature material, while it includes a default 
naive implementation of batch verification (which can be overridden).
- [`SigningKey`]: associated types for public key and signature material.
- [`Authenticator`]: associated types for private key and public key material.
- [`KeyPair`]: which includes the common get priv/pub key functions and a key-pair generation function.

## Tests and Benchmarks
There exist tests for all the three schemes, which can be run by:  
```
$ cargo test
```

One can compare all currently implemented schemes for *sign, verify, verify_batch* and 
*key-generation* by running:
```
$ cargo bench
```
