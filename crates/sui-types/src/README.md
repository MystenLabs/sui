# sui-types README

Note: this README file currently covers cryptography-related structs and methods.

Currently, three files are equipped with signature and hashing functionality (`crypto.rs`, `signature_seed` and
`messages.rs`). As of today: account signatures utilize the ed25519 scheme. Regarding validators, they still utilize the
ed25519 scheme, but we’re exploring transitioning to BLS12-381 due to native aggregation functionality.

For security purposes (i.e., the recent [Chalkias double pub-key api vulnerability](https://github.com/MystenLabs/ed25519-unsafe-libs))
and forward evolution, we wrap existing structs/functions into our own crates. In particular, our
current backing ed25519 library is `ed25519-dalek` and for BLS it's the `blst` crate; both of them have proven 
themselves as some of the fastest Rust implementations for these algorithms.
*Note that we're going to replace the [dalek-ed25519](https://github.com/dalek-cryptography/ed25519-dalek)
backing library with [ed25519-consensus](https://github.com/penumbra-zone/ed25519-consensus), which enforces co-factored
ed25519 signature verification and enables compatibility between single and batch verification.*

## Quick links

* [crypto.rs](crypto.rs), the main library for cryptography (sign/verify/hash) structs and functions.
* [signature_seed.rs](signature_seed.rs), deterministic signer using a seed, domain and some key identifier. Potential 
usage includes custodial services, in which user keys are not deterministically derived from BIP44/BIP32, but from their 
username (i.e., email address).
* [messages.rs](messages.rs), functionality for adding/verifying signatures to transactions (for both account holders 
and validators).

## Tests

Unit tests exist under the `unit_tests` folder, in particular
* `messages_tests`: to handle signed values, aggregation and certificates.
* `signature_seed_tests`: for deterministic key derivation functionality.
