// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use ed25519_dalek::ed25519::signature;
use ed25519_dalek::{ed25519, Signer};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use sui_types::base_types::SuiAddress;
use sui_types::crypto::{get_key_pair, KeyPair, Signature};

#[derive(Serialize, Deserialize)]
#[non_exhaustive]
// This will work on user signatures, but not suitable for authority signatures.
pub enum KeystoreType {
    File(PathBuf),
}

pub trait Keystore: Send + Sync {
    fn sign(&self, address: &SuiAddress, msg: &[u8]) -> Result<Signature, signature::Error>;
    fn add_random_key(&mut self) -> Result<SuiAddress, anyhow::Error>;
}

impl KeystoreType {
    pub fn init(&self) -> Result<Box<dyn Keystore>, anyhow::Error> {
        Ok(match self {
            KeystoreType::File(path) => Box::new(SuiKeystore::load_or_create(path)?),
        })
    }
}

#[derive(Serialize, Deserialize)]
pub struct SuiKeystore {
    keys: BTreeMap<SuiAddress, KeyPair>,
    path: PathBuf,
}

impl Keystore for SuiKeystore {
    fn sign(&self, address: &SuiAddress, msg: &[u8]) -> Result<Signature, signature::Error> {
        Ok(self
            .keys
            .get(address)
            .ok_or_else(|| {
                signature::Error::from_source(format!("Cannot find key for address: [{}]", address))
            })?
            .sign(msg))
    }

    fn add_random_key(&mut self) -> Result<SuiAddress, anyhow::Error> {
        let (address, keypair) = get_key_pair();
        self.keys.insert(address, keypair);
        self.save()?;
        Ok(address)
    }
}

impl SuiKeystore {
    pub fn load_or_create(path: &Path) -> Result<Self, anyhow::Error> {
        let keys: Vec<KeyPair> = if path.exists() {
            let reader = BufReader::new(File::open(path)?);
            serde_json::from_reader(reader)?
        } else {
            Vec::new()
        };

        let keys = keys
            .into_iter()
            .map(|key| (SuiAddress::from(key.public_key_bytes()), key))
            .collect();

        Ok(Self {
            keys,
            path: path.to_path_buf(),
        })
    }

    pub fn save(&self) -> Result<(), anyhow::Error> {
        let store = serde_json::to_string_pretty(&self.keys.values().collect::<Vec<_>>()).unwrap();
        Ok(fs::write(&self.path, store)?)
    }
}

pub struct SuiKeystoreSigner {
    keystore: Arc<RwLock<Box<dyn Keystore>>>,
    address: SuiAddress,
}

impl SuiKeystoreSigner {
    pub fn new(keystore: Arc<RwLock<Box<dyn Keystore>>>, account: SuiAddress) -> Self {
        Self {
            keystore,
            address: account,
        }
    }
}

impl signature::Signer<Signature> for SuiKeystoreSigner {
    fn try_sign(&self, msg: &[u8]) -> Result<Signature, ed25519::Error> {
        self.keystore
            .read()
            .unwrap()
            .sign(&self.address, msg)
            .map_err(ed25519::Error::from_source)
    }
}
