# sui-types README

Note: this README file currently covers cryptography-related structs and methods.

Currently, three files are equipped with signature and hashing functionality (`crypto.rs`, `signature_seed` and
`messages.rs`). See [Sui Signatures](../../../doc/src/learn/cryptography/sui-signatures.md) for supported signature schemes and its requirments for user and authority signatures. See [fastcrypto](https://github.com/MystenLabs/fastcrypto) for concrete implementation of various cryptography libraries.

## Quick links

- [crypto.rs](crypto.rs), the main library for cryptography (sign/verify/hash) structs and functions.
- [signature_seed.rs](signature_seed.rs), deterministic signer using a seed, domain and some key identifier. Potential
  usage includes custodial services, in which user keys are not deterministically derived from BIP44/BIP32, but from their
  username (i.e., email address).
- [messages.rs](messages.rs), functionality for adding/verifying signatures to transactions (for both account holders
  and validators).

## Tests

Unit tests exist under the `unit_tests` folder, in particular

- `messages_tests`: to handle signed values, aggregation and certificates.
- `signature_seed_tests`: for deterministic key derivation functionality.
