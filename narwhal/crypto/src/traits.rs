use base64ct::Encoding;
use eyre::eyre;
use rand::{CryptoRng, RngCore};

use serde::{de::DeserializeOwned, Serialize};
pub use signature::{Error, Signer};
use std::fmt::Debug;

// TODO: document that this is a copy of signature::Signature and why:
// we can't implement signature::Signature on Pubkeys / PrivKeys w/o violating the orphan rule,
// and we need a trait to base the definition of EncodeDecodeBase64 as an extension trait
pub trait ToFromBytes: AsRef<[u8]> + Debug + Sized {
    /// Parse a key from its byte representation
    fn from_bytes(bytes: &[u8]) -> Result<Self, Error>;

    /// Borrow a byte slice representing the serialized form of this key
    fn as_bytes(&self) -> &[u8] {
        self.as_ref()
    }
}

pub trait EncodeDecodeBase64: Sized {
    fn encode_base64(&self) -> String;
    fn decode_base64(value: &str) -> Result<Self, eyre::Report>;
}

// The Base64ct is not strictly necessary for (PubKey|Signature), but this simplifies things a lot
impl<T: ToFromBytes> EncodeDecodeBase64 for T {
    fn encode_base64(&self) -> String {
        base64ct::Base64::encode_string(self.as_bytes())
    }

    fn decode_base64(value: &str) -> Result<Self, eyre::Report> {
        let bytes = base64ct::Base64::decode_vec(value).map_err(|e| eyre!("{}", e.to_string()))?;
        <T as ToFromBytes>::from_bytes(&bytes).map_err(|e| e.into())
    }
}

pub trait VerifyingKey:
    Serialize
    + DeserializeOwned
    + std::hash::Hash
    + Eq
    + Ord
    + Default
    + ToFromBytes
    + signature::Verifier<Self::Sig>
    + Send
    + Sync
    + 'static
    + Clone
{
    type PrivKey: SigningKey<PubKey = Self>;
    type Sig: Authenticator<PubKey = Self>;

    // Expected to be overridden by implementations
    fn verify_batch(msg: &[u8], pks: &[Self], sigs: &[Self::Sig]) -> Result<(), signature::Error> {
        if pks.len() != sigs.len() {
            return Err(signature::Error::new());
        }
        pks.iter()
            .zip(sigs)
            .try_for_each(|(pk, sig)| pk.verify(msg, sig))
    }
}
pub trait SigningKey: ToFromBytes + Serialize + DeserializeOwned + Send + Sync + 'static {
    type PubKey: VerifyingKey<PrivKey = Self>;
    type Sig: Authenticator<PrivKey = Self>;
}

pub trait Authenticator:
    signature::Signature + Default + Serialize + DeserializeOwned + Send + Sync + 'static + Clone
{
    type PubKey: VerifyingKey<Sig = Self>;
    type PrivKey: SigningKey<Sig = Self>;
}

pub trait KeyPair {
    type PubKey: VerifyingKey<PrivKey = Self::PrivKey>;
    type PrivKey: SigningKey<PubKey = Self::PubKey>;
    fn public(&'_ self) -> &'_ Self::PubKey;
    fn private(self) -> Self::PrivKey;
    fn generate<R: CryptoRng + RngCore>(rng: &mut R) -> Self;
}
