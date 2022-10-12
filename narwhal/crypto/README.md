Most of the cryptographic libraries (Ed25519, Secp256k1, BLS12-381) used by Narwhal are now in [fastcrypto](https://github.com/MystenLabs/fastcrypto).

This crate only contains the implementation of BLS12-377 backed by [ark_bls12_377](https://docs.rs/ark-bls12-377/0.3.0/ark_bls12_377/) crate. Note that this implementation is under the non-default conditional compilation feature `celo`, and is not compiled or linked without that explicit flag being passed in. The goal of this implementation is to provide an experimental benchmark, it is NOT meant as a production implementation.
