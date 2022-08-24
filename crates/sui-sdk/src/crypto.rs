// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// TODO: Remove usage of rand::rngs::adapter::ReadRng.
#![allow(deprecated)]

use anyhow::anyhow;
use rand::{rngs::StdRng, SeedableRng};
use serde::{Deserialize, Serialize};
use signature::Signer;
use std::collections::BTreeMap;
use std::fmt::Write;
use std::fmt::{Display, Formatter};
use std::fs;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use bip39::Mnemonic;
use rand::rngs::adapter::ReadRng;

use sui_types::base_types::SuiAddress;
use sui_types::crypto::{
    get_key_pair_from_rng, random_key_pair_by_type_from_rng, EncodeDecodeBase64, PublicKey,
    Signature, SuiKeyPair,
};

#[derive(Serialize, Deserialize)]
#[non_exhaustive]
// This will work on user signatures, but not suitable for authority signatures.
pub enum KeystoreType {
    File(PathBuf),
    InMem(usize),
}

pub trait AccountKeystore: Send + Sync {
    fn sign(&self, address: &SuiAddress, msg: &[u8]) -> Result<Signature, signature::Error>;
    fn add_key(&mut self, keypair: SuiKeyPair) -> Result<(), anyhow::Error>;
    fn keys(&self) -> Vec<PublicKey>;
}

impl KeystoreType {
    pub fn init(&self) -> Result<SuiKeystore, anyhow::Error> {
        Ok(match self {
            KeystoreType::File(path) => SuiKeystore::from(FileBasedKeystore::load_or_create(path)?),
            KeystoreType::InMem(initial_key_number) => {
                SuiKeystore::from(InMemKeystore::new(*initial_key_number))
            }
        })
    }
}

impl Display for KeystoreType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut writer = String::new();
        match self {
            KeystoreType::File(path) => {
                writeln!(writer, "Keystore Type : File")?;
                write!(writer, "Keystore Path : {:?}", path)?;
                write!(f, "{}", writer)
            }
            KeystoreType::InMem(_) => {
                writeln!(writer, "Keystore Type : InMem")?;
                write!(f, "{}", writer)
            }
        }
    }
}

#[derive(Default)]
pub struct FileBasedKeystore {
    keys: BTreeMap<SuiAddress, SuiKeyPair>,
    path: Option<PathBuf>,
}

impl AccountKeystore for FileBasedKeystore {
    fn sign(&self, address: &SuiAddress, msg: &[u8]) -> Result<Signature, signature::Error> {
        self.keys
            .get(address)
            .ok_or_else(|| {
                signature::Error::from_source(format!("Cannot find key for address: [{address}]"))
            })?
            .try_sign(msg)
    }

    fn add_key(&mut self, keypair: SuiKeyPair) -> Result<(), anyhow::Error> {
        let address: SuiAddress = (&keypair.public()).into();
        self.keys.insert(address, keypair);
        self.save()?;
        Ok(())
    }

    fn keys(&self) -> Vec<PublicKey> {
        self.keys.values().map(|key| key.public()).collect()
    }
}

impl FileBasedKeystore {
    pub fn load_or_create(path: &Path) -> Result<Self, anyhow::Error> {
        let keys = if path.exists() {
            let reader = BufReader::new(File::open(path)?);
            let kp_strings: Vec<String> = serde_json::from_reader(reader)?;
            kp_strings
                .iter()
                .map(|kpstr| {
                    let key = SuiKeyPair::decode_base64(kpstr);
                    key.map(|k| (Into::<SuiAddress>::into(&k.public()), k))
                })
                .collect::<Result<BTreeMap<_, _>, _>>()
                .map_err(|e| anyhow::anyhow!("Invalid Keypair file {:#?} {:?}", e, path))?
        } else {
            BTreeMap::new()
        };

        Ok(Self {
            keys,
            path: Some(path.to_path_buf()),
        })
    }

    pub fn set_path(&mut self, path: &Path) {
        self.path = Some(path.to_path_buf());
    }

    pub fn save(&self) -> Result<(), anyhow::Error> {
        if let Some(path) = &self.path {
            let store = serde_json::to_string_pretty(
                &self
                    .keys
                    .values()
                    .map(EncodeDecodeBase64::encode_base64)
                    .collect::<Vec<_>>(),
            )
            .unwrap();
            fs::write(path, store)?
        }
        Ok(())
    }

    pub fn key_pairs(&self) -> Vec<&SuiKeyPair> {
        self.keys.values().collect()
    }
}

pub struct SuiKeystore(Box<dyn AccountKeystore>);

impl SuiKeystore {
    fn from<S: AccountKeystore + 'static>(keystore: S) -> Self {
        Self(Box::new(keystore))
    }

    pub fn add_key(&mut self, keypair: SuiKeyPair) -> Result<(), anyhow::Error> {
        self.0.add_key(keypair)
    }

    pub fn generate_new_key(
        &mut self,
        key_scheme: Option<String>,
    ) -> Result<(SuiAddress, String, u8), anyhow::Error> {
        let mnemonic = Mnemonic::generate(12)?;
        let seed = mnemonic.to_seed("");
        let mut rng = RngWrapper(ReadRng::new(&seed));
        match random_key_pair_by_type_from_rng(key_scheme, &mut rng) {
            Ok((address, kp)) => {
                let flag = kp.public().flag();
                self.0.add_key(kp)?;
                Ok((address, mnemonic.to_string(), flag))
            }
            Err(e) => Err(anyhow!("error generating key {:?}", e)),
        }
    }

    pub fn keys(&self) -> Vec<PublicKey> {
        self.0.keys()
    }

    pub fn addresses(&self) -> Vec<SuiAddress> {
        self.keys().iter().map(|k| k.into()).collect()
    }

    pub fn signer(&self, signer: SuiAddress) -> impl Signer<Signature> + '_ {
        KeystoreSigner::new(&*self.0, signer)
    }

    pub fn import_from_mnemonic(
        &mut self,
        phrase: &str,
        key_scheme: Option<String>,
    ) -> Result<SuiAddress, anyhow::Error> {
        let seed = &Mnemonic::from_str(phrase).unwrap().to_seed("");
        let mut rng = RngWrapper(ReadRng::new(seed));
        match random_key_pair_by_type_from_rng(key_scheme, &mut rng) {
            Ok((address, kp)) => {
                self.0.add_key(kp)?;
                Ok(address)
            }
            Err(e) => Err(anyhow!("error getting keypair {:?}", e)),
        }
    }

    pub fn sign(&self, address: &SuiAddress, msg: &[u8]) -> Result<Signature, signature::Error> {
        self.0.sign(address, msg)
    }
}

/// wrapper for adding CryptoRng and RngCore impl to ReadRng.
struct RngWrapper<'a>(ReadRng<&'a [u8]>);

impl rand::CryptoRng for RngWrapper<'_> {}
impl rand::RngCore for RngWrapper<'_> {
    fn next_u32(&mut self) -> u32 {
        self.0.next_u32()
    }

    fn next_u64(&mut self) -> u64 {
        self.0.next_u64()
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        self.0.fill_bytes(dest)
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand::Error> {
        self.0.try_fill_bytes(dest)
    }
}

struct KeystoreSigner<'a> {
    keystore: &'a dyn AccountKeystore,
    address: SuiAddress,
}

impl<'a> KeystoreSigner<'a> {
    pub fn new(keystore: &'a dyn AccountKeystore, account: SuiAddress) -> Self {
        Self {
            keystore,
            address: account,
        }
    }
}

impl Signer<Signature> for KeystoreSigner<'_> {
    fn try_sign(&self, msg: &[u8]) -> Result<Signature, signature::Error> {
        self.keystore.sign(&self.address, msg)
    }
}

#[derive(Default)]
struct InMemKeystore {
    keys: BTreeMap<SuiAddress, SuiKeyPair>,
}

impl AccountKeystore for InMemKeystore {
    fn sign(&self, address: &SuiAddress, msg: &[u8]) -> Result<Signature, signature::Error> {
        self.keys
            .get(address)
            .ok_or_else(|| {
                signature::Error::from_source(format!("Cannot find key for address: [{address}]"))
            })?
            .try_sign(msg)
    }

    fn add_key(&mut self, keypair: SuiKeyPair) -> Result<(), anyhow::Error> {
        let address: SuiAddress = (&keypair.public()).into();
        self.keys.insert(address, keypair);
        Ok(())
    }

    fn keys(&self) -> Vec<PublicKey> {
        self.keys.values().map(|key| key.public()).collect()
    }
}

impl InMemKeystore {
    pub fn new(initial_key_number: usize) -> Self {
        let mut rng = StdRng::from_seed([0; 32]);
        let keys = (0..initial_key_number)
            .map(|_| get_key_pair_from_rng(&mut rng))
            .map(|(ad, k)| (ad, SuiKeyPair::Ed25519SuiKeyPair(k)))
            .collect::<BTreeMap<SuiAddress, SuiKeyPair>>();

        Self { keys }
    }
}

impl AccountKeystore for Box<dyn AccountKeystore> {
    fn sign(&self, address: &SuiAddress, msg: &[u8]) -> Result<Signature, signature::Error> {
        (**self).sign(address, msg)
    }

    fn add_key(&mut self, keypair: SuiKeyPair) -> Result<(), anyhow::Error> {
        (**self).add_key(keypair)
    }

    fn keys(&self) -> Vec<PublicKey> {
        (**self).keys()
    }
}
