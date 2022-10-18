// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use bip32::DerivationPath;
use bip39::{Language, Mnemonic, MnemonicType, Seed};
use rand::{rngs::StdRng, SeedableRng};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use signature::Signer;
use std::collections::BTreeMap;
use std::fmt::Write;
use std::fmt::{Display, Formatter};
use std::fs;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use sui_types::base_types::SuiAddress;
use sui_types::crypto::{
    enum_dispatch, get_key_pair_from_rng, EncodeDecodeBase64, PublicKey, Signature,
    SignatureScheme, SuiKeyPair,
};

use crate::key_derive::derive_key_pair_from_path;

#[derive(Serialize, Deserialize)]
#[enum_dispatch(AccountKeystore)]
pub enum Keystore {
    File(FileBasedKeystore),
    InMem(InMemKeystore),
}
#[enum_dispatch]
pub trait AccountKeystore: Send + Sync {
    fn sign(&self, address: &SuiAddress, msg: &[u8]) -> Result<Signature, signature::Error>;
    fn add_key(&mut self, keypair: SuiKeyPair) -> Result<(), anyhow::Error>;
    fn keys(&self) -> Vec<PublicKey>;
    fn get_key(&self, address: &SuiAddress) -> Result<&SuiKeyPair, anyhow::Error>;
    fn addresses(&self) -> Vec<SuiAddress> {
        self.keys().iter().map(|k| k.into()).collect()
    }

    fn generate_new_key(
        &mut self,
        key_scheme: SignatureScheme,
        derivation_path: Option<DerivationPath>,
    ) -> Result<(SuiAddress, String, SignatureScheme), anyhow::Error> {
        let mnemonic = Mnemonic::new(MnemonicType::Words12, Language::English);
        match derive_key_pair_from_path(
            Seed::new(&mnemonic, "").as_bytes(),
            derivation_path,
            &key_scheme,
        ) {
            Ok((address, keypair)) => {
                self.add_key(keypair)?;
                Ok((address, mnemonic.phrase().to_string(), key_scheme))
            }
            Err(e) => Err(anyhow!("error generating key {:?}", e)),
        }
    }

    fn import_from_mnemonic(
        &mut self,
        phrase: &str,
        key_scheme: SignatureScheme,
        derivation_path: Option<DerivationPath>,
    ) -> Result<SuiAddress, anyhow::Error> {
        let mnemonic = Mnemonic::from_phrase(phrase, Language::English)
            .map_err(|e| anyhow::anyhow!("Invalid mnemonic phrase: {:?}", e))?;
        let seed = Seed::new(&mnemonic, "");
        match derive_key_pair_from_path(seed.as_bytes(), derivation_path, &key_scheme) {
            Ok((address, kp)) => {
                self.add_key(kp)?;
                Ok(address)
            }
            Err(e) => Err(anyhow!("error getting keypair {:?}", e)),
        }
    }
}

impl Display for Keystore {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut writer = String::new();
        match self {
            Keystore::File(file) => {
                writeln!(writer, "Keystore Type : File")?;
                write!(writer, "Keystore Path : {:?}", file.path)?;
                write!(f, "{}", writer)
            }
            Keystore::InMem(_) => {
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

impl Serialize for FileBasedKeystore {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(
            self.path
                .as_ref()
                .unwrap_or(&PathBuf::default())
                .to_str()
                .unwrap_or(""),
        )
    }
}

impl<'de> Deserialize<'de> for FileBasedKeystore {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::Error;
        FileBasedKeystore::new(&PathBuf::from(String::deserialize(deserializer)?))
            .map_err(D::Error::custom)
    }
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

    fn get_key(&self, address: &SuiAddress) -> Result<&SuiKeyPair, anyhow::Error> {
        match self.keys.get(address) {
            Some(key) => Ok(key),
            None => Err(anyhow!("Cannot find key for address: [{address}]")),
        }
    }
}

impl FileBasedKeystore {
    pub fn new(path: &PathBuf) -> Result<Self, anyhow::Error> {
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

#[derive(Default, Serialize, Deserialize)]
pub struct InMemKeystore {
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

    fn get_key(&self, address: &SuiAddress) -> Result<&SuiKeyPair, anyhow::Error> {
        match self.keys.get(address) {
            Some(key) => Ok(key),
            None => Err(anyhow!("Cannot find key for address: [{address}]")),
        }
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
