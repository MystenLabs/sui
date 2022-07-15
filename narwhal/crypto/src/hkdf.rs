// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::traits::{KeyPair, SigningKey, ToFromBytes, DEFAULT_DOMAIN};
use digest::{
    block_buffer::Eager,
    consts::U256,
    core_api::{BlockSizeUser, BufferKindUser, CoreProxy, FixedOutputCore, UpdateCore},
    typenum::{IsLess, Le, NonZero, Unsigned},
    HashMarker, OutputSizeUser,
};
use hkdf::hmac::Hmac;

/// Creation of a keypair using the [RFC 5869](https://tools.ietf.org/html/rfc5869) HKDF specification.
/// This requires choosing an HMAC function of the correct length (conservatively, the size of a private key for this curve).
/// Despite the unsightly generics (which aim to ensure this works for a wide range of hash functions), this is straightforward to use.
///
/// Example:
/// ```rust
/// use sha3::Sha3_256;
/// use crypto::ed25519::Ed25519KeyPair;
/// use crypto::hkdf::hkdf_generate_from_ikm;
/// # fn main() {
///     let ikm = b"some_ikm";
///     let domain = b"my_app";
///     let salt = b"some_salt";
///     let my_keypair = hkdf_generate_from_ikm::<Sha3_256, Ed25519KeyPair>(ikm, salt, Some(domain));
///
///     let my_keypair_default_domain = hkdf_generate_from_ikm::<Sha3_256, Ed25519KeyPair>(ikm, salt, None);
/// # }
/// ```
///
/// Note: This HKDF function may not match the native library's deterministic key generation functions.
/// For example, observe that in the blst library:
/// ```rust
/// use sha3::Sha3_256;
/// use crypto::bls12381::BLS12381KeyPair;
/// use crypto::traits::{KeyPair, SigningKey, ToFromBytes};
/// use crypto::hkdf::hkdf_generate_from_ikm;
///
/// # fn main() {
///     let ikm = b"02345678001234567890123456789012";
///     let domain = b"my_app";
///     let salt = b"some_salt";

///     let my_keypair = hkdf_generate_from_ikm::<Sha3_256, BLS12381KeyPair>(ikm, salt, Some(domain)).unwrap();
///     let native_sk = blst::min_sig::SecretKey::key_gen_v4_5(ikm, salt, domain).unwrap();

///     assert_ne!(native_sk.to_bytes(), my_keypair.private().as_bytes());
/// # }
/// ```
pub fn hkdf_generate_from_ikm<'a, H: OutputSizeUser, K: KeyPair>(
    ikm: &[u8],             // IKM (32 bytes)
    salt: &[u8],            // Optional salt
    info: Option<&'a [u8]>, // Optional domain
) -> Result<K, signature::Error>
where
    // This is a tad tedious, because of hkdf's use of a sealed trait. But mostly harmless.
    H: CoreProxy + OutputSizeUser,
    H::Core: HashMarker
        + UpdateCore
        + FixedOutputCore
        + BufferKindUser<BufferKind = Eager>
        + Default
        + Clone,
    <H::Core as BlockSizeUser>::BlockSize: IsLess<U256>,
    Le<<H::Core as BlockSizeUser>::BlockSize, U256>: NonZero,
{
    let info = info.unwrap_or(&DEFAULT_DOMAIN);
    let hk = hkdf::Hkdf::<H, Hmac<H>>::new(Some(salt), ikm);

    // we need the HKDF to be able to expand precisely to the byte length of a Private key for the chosen KeyPair parameter.
    // This check is a tad over constraining (check Hkdf impl for a more relaxed variant) but is always correct.
    if H::OutputSize::USIZE != K::PrivKey::LENGTH {
        return Err(signature::Error::from_source(hkdf::InvalidLength));
    }

    let mut okm = vec![0u8; K::PrivKey::LENGTH];
    hk.expand(info, &mut okm)
        .map_err(|_| signature::Error::new())?;

    let secret_key = K::PrivKey::from_bytes(&okm[..]).map_err(|_| signature::Error::new())?;

    let keypair = K::from(secret_key);
    Ok(keypair)
}
