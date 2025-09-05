// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::keystore::{
    validate_alias, AccountKeystore, Alias, GenerateOptions, GeneratedKey, ALIASES_FILE_EXTENSION,
};
use crate::random_names::random_name;

use anyhow::{anyhow, bail};
use anyhow::{Context, Error};
use async_trait::async_trait;
use base64;
use base64::{engine::general_purpose, Engine as _};
use bcs;
use fastcrypto::traits::{EncodeDecodeBase64, VerifyingKey};
use jsonrpc::client::Endpoint;
use mockall::{automock, predicate::*};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{json, Value as JsonValue};
use shared_crypto::intent::{Intent, IntentMessage};
use std::collections::{BTreeMap, HashSet};
use std::fmt::Debug;
use std::path::PathBuf;
use std::process::Stdio;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::{PublicKey, Signature, SuiKeyPair, SuiSignature, SuiSignatureInner};
use tokio::process::Command;

#[derive(Debug)]
/// External keystore for managing keys and aliases with an external signer.
pub struct External {
    /// Holds a map of addresses to aliases
    pub aliases: BTreeMap<SuiAddress, Alias>,
    // Holds a map of addresses to [`StoredKey`]
    pub keys: BTreeMap<SuiAddress, StoredKey>,
    command_runner: Box<dyn CommandRunner>,
    path: Option<PathBuf>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StoredKey {
    pub public_key: PublicKey,
    /// External signer binary (e.g., "sui-ledger-signer")
    pub ext_signer: String,
    /// Key ID for the external signer, used to query/interact with keys on the external signer,
    /// derivation path, AWS ARN, etc.
    pub key_id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ExternalKey {
    pub public_key: PublicKey,
    pub key_id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct KeysResponse {
    pub keys: Vec<ExternalKey>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
/// Request structure for signing a message with an external signer.
pub struct SignRequest {
    /// Key ID for the external signer, used to identify which key to use for signing.
    pub key_id: String,
    /// base64 encoded message to sign
    pub msg: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intent: Option<Intent>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SignResponse {
    pub signature: Signature,
}

#[automock]
#[async_trait]
pub trait CommandRunner: Send + Sync + Debug {
    async fn run(&self, command: &str, method: &str, args: JsonValue) -> Result<JsonValue, Error>;
}

#[derive(Debug)]
struct StdCommandRunner;
#[async_trait]
impl CommandRunner for StdCommandRunner {
    async fn run(&self, command: &str, method: &str, args: JsonValue) -> Result<JsonValue, Error> {
        let mut cmd = Command::new(command)
            .arg("call")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;

        let mut endpoint = Endpoint::new(
            cmd.stdout.take().expect("No stdout"),
            cmd.stdin.take().expect("No stdin"),
        );

        let res: JsonValue = endpoint.call(method, args).await?;
        if res.is_null() {
            return Err(anyhow!("Command returned null result"));
        }

        let output = cmd
            .wait_with_output()
            .await
            .map_err(|e| anyhow!("Failed to wait for command to finish: {}", e))?;

        if !output.status.success() {
            return Err(Error::msg(format!(
                "Command failed with status: {}",
                output.status
            )));
        }

        if !res["error"].is_null() {
            return Err(anyhow!("Command failed with error: {:?}", res["error"]));
        }

        Ok(res)
    }
}

impl External {
    /// Load keys and aliases from a given path or creates a new External keystore.
    pub fn load_or_create(path: &PathBuf) -> Result<Self, Error> {
        let mut aliases_store_directory = path.clone();
        aliases_store_directory.set_extension(ALIASES_FILE_EXTENSION);
        let aliases: BTreeMap<SuiAddress, Alias> = if aliases_store_directory.exists() {
            let aliases_store: String = std::fs::read_to_string(&aliases_store_directory)
                .map_err(|e| anyhow!("Failed to read aliases file: {}", e))?;
            serde_json::from_str(&aliases_store)
                .map_err(|e| anyhow!("Failed to parse aliases file: {}", e))?
        } else {
            BTreeMap::default()
        };

        let keys: BTreeMap<SuiAddress, StoredKey> = if path.exists() {
            let keys_store: String = std::fs::read_to_string(path)
                .map_err(|e| anyhow!("Failed to read keys file: {}", e))?;
            serde_json::from_str(&keys_store)
                .map_err(|e| anyhow!("Failed to parse keys file: {}", e))?
        } else {
            BTreeMap::default()
        };

        Ok(Self {
            aliases,
            keys,
            command_runner: Box::new(StdCommandRunner),
            path: Some(path.clone()),
        })
    }

    pub fn from_existing(old: &mut Self) -> Self {
        Self {
            aliases: old.aliases.clone(),
            keys: old.keys.clone(),
            command_runner: Box::new(StdCommandRunner),
            path: old.path.clone(),
        }
    }

    /// Test function for mocked command runner
    pub fn new_for_test(command_runner: Box<dyn CommandRunner>, path: Option<PathBuf>) -> Self {
        Self {
            aliases: BTreeMap::default(),
            keys: BTreeMap::default(),
            command_runner,
            path,
        }
    }

    /// Execute a command against the command runner
    pub async fn exec(
        &self,
        command: &str,
        method: &str,
        args: JsonValue,
    ) -> Result<JsonValue, Error> {
        self.command_runner.run(command, method, args).await
    }

    /// Add a Key ID from the given an external signer to the Sui CLI index
    pub async fn add_existing(
        &mut self,
        ext_signer: String,
        key_id: String,
    ) -> Result<StoredKey, Error> {
        let keys = self.signer_available_keys(ext_signer.clone()).await?;

        let key: StoredKey = keys
            .into_iter()
            .find(|k| k.key_id == key_id)
            .ok_or_else(|| {
                anyhow!(
                    "Key with id {} not found for external signer {}",
                    key_id,
                    ext_signer
                )
            })?;

        self.keys.insert((&key.public_key).into(), key.clone());
        self.save().await?;
        Ok(key)
    }

    /// Return all Key IDs associated with an external signer, indexed or not
    pub async fn signer_available_keys(&self, ext_signer: String) -> Result<Vec<StoredKey>, Error> {
        let result = self.exec(&ext_signer, "keys", json![null]).await?;

        let keys_response: KeysResponse = serde_json::from_value(result)
            .map_err(|e| anyhow!("Failed to parse keys response: {}", e))?;

        let mut keys = Vec::new();
        for external_key in keys_response.keys.into_iter() {
            keys.push(StoredKey {
                key_id: external_key.key_id,
                public_key: external_key.public_key,
                ext_signer: ext_signer.clone(),
            });
        }
        Ok(keys)
    }

    pub fn is_indexed(&self, key: &StoredKey) -> bool {
        self.keys.contains_key(&SuiAddress::from(&key.public_key))
    }

    pub async fn save_aliases(&self) -> Result<(), Error> {
        if let Some(path) = &self.path {
            let aliases_store: String = serde_json::to_string_pretty(&self.aliases)
                .map_err(|e| anyhow!("Serialization error: {}", e))?;

            let mut aliases_path = path.clone();
            aliases_path.set_extension(ALIASES_FILE_EXTENSION);
            tokio::task::spawn_blocking(move || std::fs::write(aliases_path, aliases_store))
                .await?
                .with_context(|| {
                    format!(
                        "Cannot write aliases to file: {}",
                        path.with_extension(ALIASES_FILE_EXTENSION).display()
                    )
                })?;
            Ok(())
        } else {
            Err(anyhow!("Path is not set for External keystore"))
        }
    }

    pub async fn save_keystore(&self) -> Result<(), Error> {
        if let Some(path) = &self.path {
            let store: String = serde_json::to_string_pretty(&self.keys)
                .map_err(|e| anyhow!("Serialization error: {}", e))?;

            let keystore_path = path.clone();
            tokio::task::spawn_blocking(move || std::fs::write(keystore_path, store))
                .await?
                .with_context(|| format!("Cannot write keystore to file: {}", path.display()))?;
            Ok(())
        } else {
            Err(anyhow!("Path is not set for External keystore"))
        }
    }

    pub async fn save(&self) -> Result<(), Error> {
        self.save_aliases().await?;
        self.save_keystore().await?;
        Ok(())
    }
}

impl Serialize for External {
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

impl<'de> Deserialize<'de> for External {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::Error;
        External::load_or_create(&PathBuf::from(String::deserialize(deserializer)?))
            .map_err(D::Error::custom)
    }
}

#[async_trait]
impl AccountKeystore for External {
    async fn generate(
        &mut self,
        alias: Option<String>,
        opts: GenerateOptions,
    ) -> Result<GeneratedKey, Error> {
        let GenerateOptions::ExternalSigner(ext_signer) = opts else {
            return Err(anyhow!("Signer must be provided for external keys."));
        };

        let res = self.exec(&ext_signer, "create_key", json![null]).await?;
        let ExternalKey { key_id, public_key } = serde_json::from_value(res)
            .map_err(|e| anyhow!("Failed to parse key response: {}", e))?;
        let address: SuiAddress = (&public_key).into();

        self.keys.insert(
            address,
            StoredKey {
                public_key: public_key.clone(),
                ext_signer,
                key_id: key_id.to_string(),
            },
        );

        self.aliases.insert(
            address,
            Alias {
                alias: self.create_alias(alias)?,
                public_key_base64: public_key.encode_base64(),
            },
        );

        let scheme = public_key.scheme();
        Ok(GeneratedKey {
            address,
            public_key,
            scheme,
        })
    }

    /// Import a keypair into the keystore.
    async fn import(&mut self, _alias: Option<String>, _keypair: SuiKeyPair) -> Result<(), Error> {
        Err(anyhow!("Import not supported for external keys."))
    }

    async fn remove(&mut self, address: SuiAddress) -> Result<(), Error> {
        self.aliases.remove(&address);
        self.keys.remove(&address);
        self.save().await?;
        Ok(())
    }

    fn entries(&self) -> Vec<PublicKey> {
        let mut keys = Vec::new();
        for StoredKey { public_key, .. } in self.keys.values() {
            keys.push(public_key.clone());
        }
        keys
    }

    /// Export the key pair for the given address as a `SuiKeyPair`.
    fn export(&self, _address: &SuiAddress) -> Result<&SuiKeyPair, Error> {
        Err(anyhow!("Export not supported for external keys."))
    }

    async fn sign_hashed(
        &self,
        address: &SuiAddress,
        msg: &[u8],
    ) -> Result<Signature, signature::Error> {
        let StoredKey {
            key_id,
            ext_signer,
            public_key,
        } = self
            .keys
            .get(address)
            .ok_or_else(|| {
                signature::Error::from_source(anyhow!("Key corresponding to {address} not found"))
            })?
            .clone();

        let sign_request: SignRequest = SignRequest {
            key_id,
            msg: general_purpose::STANDARD.encode(msg),
            intent: None,
        };
        let result = self
            .exec(
                &ext_signer,
                "sign_hashed",
                serde_json::to_value(&sign_request).map_err(|e| {
                    signature::Error::from_source(anyhow!(
                        "Failed to serialize sign request: {}",
                        e
                    ))
                })?,
            )
            .await
            .map_err(signature::Error::from_source)?;

        let result: SignResponse = serde_json::from_value(result).map_err(|e| {
            signature::Error::from_source(anyhow!("Failed to parse sign response: {}", e))
        })?;

        let address = SuiAddress::from(&public_key);
        match &result.signature {
            Signature::Ed25519SuiSignature(s) => {
                let (sig, pk) = s.get_verification_inputs().map_err(|e| {
                    signature::Error::from_source(anyhow!(
                        "Failed to get verification inputs: {}",
                        e
                    ))
                })?;
                let signature_address = SuiAddress::from(&pk);
                if signature_address != address {
                    return Err(signature::Error::from_source(anyhow!(
                        "Signature address {} does not match expected address {}",
                        signature_address,
                        address
                    )));
                }

                pk.verify(msg, &sig).map_err(|e| {
                    signature::Error::from_source(anyhow!("Signature verification failed: {}", e))
                })?;
            }
            Signature::Secp256k1SuiSignature(s) => {
                let (sig, pk) = s.get_verification_inputs().map_err(|e| {
                    signature::Error::from_source(anyhow!(
                        "Failed to get verification inputs: {}",
                        e
                    ))
                })?;
                let signature_address = SuiAddress::from(&pk);
                if signature_address != address {
                    return Err(signature::Error::from_source(anyhow!(
                        "Signature address {} does not match expected address {}",
                        signature_address,
                        address
                    )));
                }

                pk.verify(msg, &sig).map_err(|e| {
                    signature::Error::from_source(anyhow!("Signature verification failed: {}", e))
                })?;
            }
            Signature::Secp256r1SuiSignature(s) => {
                let (sig, pk) = s.get_verification_inputs().map_err(|e| {
                    signature::Error::from_source(anyhow!(
                        "Failed to get verification inputs: {}",
                        e
                    ))
                })?;
                let signature_address = SuiAddress::from(&pk);
                if signature_address != address {
                    return Err(signature::Error::from_source(anyhow!(
                        "Signature address {} does not match expected address {}",
                        signature_address,
                        address
                    )));
                }

                pk.verify(msg, &sig).map_err(|e| {
                    signature::Error::from_source(anyhow!("Signature verification failed: {}", e))
                })?;
            }
        }

        Ok(result.signature)
    }

    async fn sign_secure<T>(
        &self,
        address: &SuiAddress,
        msg: &T,
        intent: Intent,
    ) -> Result<Signature, signature::Error>
    where
        T: Serialize + Sync,
    {
        let StoredKey {
            key_id,
            ext_signer,
            public_key,
        } = self
            .keys
            .get(address)
            .ok_or_else(|| {
                signature::Error::from_source(anyhow!("Key corresponding to {address} not found"))
            })?
            .clone();

        let msg_bcs = general_purpose::STANDARD.encode(&bcs::to_bytes(&msg).map_err(|e| {
            signature::Error::from_source(anyhow!("Failed to serialize message: {}", e))
        })?);

        let sign_request = SignRequest {
            key_id,
            msg: msg_bcs,
            intent: Some(intent),
        };

        let result = self
            .exec(
                &ext_signer,
                "sign",
                serde_json::to_value(&sign_request).map_err(|e| {
                    signature::Error::from_source(anyhow!(
                        "Failed to serialize sign request: {}",
                        e
                    ))
                })?,
            )
            .await
            .map_err(|e| signature::Error::from_source(anyhow!("Failed to sign message: {}", e)))?;

        let result: SignResponse = serde_json::from_value(result).map_err(|e| {
            signature::Error::from_source(anyhow!("Failed to parse sign response: {}", e))
        })?;

        let intent_message = IntentMessage::new(sign_request.intent.unwrap(), msg);
        if let Err(e) =
            result
                .signature
                .verify_secure(&intent_message, *address, public_key.scheme())
        {
            return Err(signature::Error::from_source(anyhow!(
                "Signature verification failed: {}",
                e
            )));
        }

        Ok(result.signature)
    }

    fn addresses_with_alias(&self) -> Vec<(&SuiAddress, &Alias)> {
        let mut addresses = Vec::new();
        for (address, alias) in &self.aliases {
            addresses.push((address, alias));
        }
        addresses
    }

    fn aliases(&self) -> Vec<&Alias> {
        let mut aliases = Vec::new();
        for alias in self.aliases.values() {
            aliases.push(alias);
        }
        aliases
    }

    fn aliases_mut(&mut self) -> Vec<&mut Alias> {
        let mut aliases = Vec::new();
        for alias in self.aliases.values_mut() {
            aliases.push(alias);
        }
        aliases
    }

    fn get_alias(&self, address: &SuiAddress) -> Result<String, anyhow::Error> {
        match self.aliases.get(address) {
            Some(alias) => Ok(alias.alias.clone()),
            None => bail!("Cannot find alias for address {address}"),
        }
    }

    fn create_alias(&self, alias: Option<String>) -> Result<String, Error> {
        match alias {
            Some(a) if self.alias_exists(&a) => {
                bail!("Alias {a} already exists. Please choose another alias.")
            }
            Some(a) => validate_alias(&a),
            None => Ok(random_name(
                &self
                    .aliases()
                    .into_iter()
                    .map(|x| x.alias.to_string())
                    .collect::<HashSet<_>>(),
            )),
        }
    }

    async fn update_alias(
        &mut self,
        old_alias: &str,
        new_alias: Option<&str>,
    ) -> Result<String, Error> {
        let new_alias_name = self.update_alias_value(old_alias, new_alias)?;
        self.save_aliases().await?;
        Ok(new_alias_name)
    }
}

#[cfg(test)]
mod tests {
    use super::{External, MockCommandRunner, StdCommandRunner, StoredKey};
    use crate::key_identity::KeyIdentity;
    use crate::keystore::{AccountKeystore, GenerateOptions, GeneratedKey};
    use anyhow::anyhow;
    use fastcrypto::ed25519::Ed25519KeyPair;
    use fastcrypto::secp256k1::Secp256k1KeyPair;
    use fastcrypto::traits::{EncodeDecodeBase64, KeyPair, ToFromBytes};
    use mockall::predicate::eq;
    use rand::prelude::StdRng;
    use rand::{thread_rng, SeedableRng};
    use serde_json::Value as JsonValue;
    use serde_json::{json, Value};
    use shared_crypto::intent::{Intent, IntentMessage};
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use std::str::FromStr;
    use sui_types::base_types::SuiAddress;
    use sui_types::crypto::SignatureScheme::{Secp256k1, ED25519};
    use sui_types::crypto::{PublicKey, Signature, SuiKeyPair};
    use tempfile::TempDir;

    const PUBLIC_KEY: &str = "ALJ0GaLcBTTwTTh5dvyc6xaxwrjkG1spQzlL+W4CGLqG";
    const UNTAGGED_PUBLIC_KEY: &str = "snQZotwFNPBNOHl2/JzrFrHCuOQbWylDOUv5bgIYuoY=";
    const ADDRESS: &str = "0x9219616732544c54259b3f5aeef5ec078535e322ee63f7de2ca8a197fd2a4f6f";

    fn load_external_keystore() -> External {
        let cargo_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap())
            .join("src")
            .join("unit_tests")
            .join("fixtures")
            .join("external_config")
            .join("external.keystore");

        External::load_or_create(&cargo_dir).expect("Failed to load external keystore")
    }

    #[test]
    fn test_load_new_from_path() {
        let cargo_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap())
            .join("unit_tests")
            .join("fixtures")
            .join("external_config");

        let external = External::load_or_create(&cargo_dir).unwrap();

        assert!(external.aliases.is_empty());
        assert!(external.keys.is_empty());
        assert!(external.path.is_some());
    }

    #[tokio::test]
    async fn test_serialize() {
        let tmp_dir = TempDir::new().unwrap();
        let path = tmp_dir.path().join("external.keystore");

        let mut external = External {
            aliases: BTreeMap::default(),
            keys: BTreeMap::default(),
            command_runner: Box::new(StdCommandRunner),
            path: Some(path.clone()),
        };

        // Add key
        external.keys.insert(
            SuiAddress::from_str(ADDRESS).unwrap(),
            StoredKey {
                public_key: PublicKey::decode_base64(PUBLIC_KEY).unwrap(),
                ext_signer: "signer".to_string(),
                key_id: "key-123".to_string(),
            },
        );
        // Add alias
        external.aliases.insert(
            SuiAddress::from_str(ADDRESS).unwrap(),
            crate::keystore::Alias {
                alias: "test_alias".to_string(),
                public_key_base64: PUBLIC_KEY.to_string(),
            },
        );

        external.save().await.unwrap();

        // custom serializer
        let serialized = serde_json::to_string(&external).expect("Failed to serialize external");

        assert!(!serialized.is_empty());
        assert!(serialized.contains("/external.keystore"));
        // deserialize back to External via custom deserializer
        let deserialized: External = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized.keys.len(), 1);
        assert_eq!(deserialized.aliases.len(), 1);

        // malformed keystore
        let external_bad_keystore_path =
            PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap())
                .join("src")
                .join("unit_tests")
                .join("fixtures")
                .join("external_config")
                .join("external_bad_keystore.keystore");

        let external_bad = External::load_or_create(&external_bad_keystore_path);
        assert!(
            external_bad.is_err(),
            "Expected error when loading from bad keystore path"
        );

        // malformed aliases
        let external_bad_aliases_path = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap())
            .join("src")
            .join("unit_tests")
            .join("fixtures")
            .join("external_config")
            .join("external_bad_aliases.keystore");
        let external_bad_aliases = External::load_or_create(&external_bad_aliases_path);
        assert!(
            external_bad_aliases.is_err(),
            "Expected error when loading from bad aliases path"
        );
    }

    #[tokio::test]
    async fn test_exec() {
        let mut mock = MockCommandRunner::new();
        mock.expect_run()
            .with(
                eq("sui-key-tool"),
                eq("test_method"),
                eq(json!(["arg1", "arg2"])),
            )
            .times(1)
            .returning(|_, _, _| Ok(JsonValue::Null));

        let external = External::new_for_test(Box::new(mock), None);
        let args = json!(["arg1", "arg2"]);
        assert!(external
            .exec("sui-key-tool", "test_method", args)
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn test_create_key_success() {
        let mut mock = MockCommandRunner::new();
        let key_id = "key-123";
        mock.expect_run()
            .with(eq("signer"), eq("create_key"), eq(json![null]))
            .returning(move |_, _, _| {
                Ok(json!({
                    "key_id": key_id,
                    "public_key": {
                        "Ed25519": UNTAGGED_PUBLIC_KEY
                    }
                }))
            });
        let mut external = External::new_for_test(Box::new(mock), None);
        let result = external
            .generate(None, GenerateOptions::ExternalSigner("signer".to_string()))
            .await;
        assert!(result.is_ok());
        let GeneratedKey {
            address,
            public_key,
            scheme,
        } = result.unwrap();
        assert_eq!(scheme, ED25519);
        assert_eq!(address, SuiAddress::from_str(ADDRESS).unwrap());
        assert_eq!(public_key.encode_base64(), PUBLIC_KEY);
        assert!(external.keys.contains_key(&address));

        // With a different signature scheme
        let secp256k1_key =
            SuiKeyPair::Secp256k1(Secp256k1KeyPair::generate(&mut StdRng::from_seed([0; 32])));
        let public_key_json_value: Value = serde_json::to_value(secp256k1_key.public()).unwrap();

        let mut mock = MockCommandRunner::new();
        mock.expect_run()
            .with(eq("signer"), eq("create_key"), eq(json![null]))
            .returning(move |_, _, _| {
                Ok(json!({
                    "key_id": key_id,
                    "public_key": public_key_json_value,
                }))
            });

        let mut external = External::new_for_test(Box::new(mock), None);
        let result = external
            .generate(None, GenerateOptions::ExternalSigner("signer".to_string()))
            .await;
        let GeneratedKey { scheme, .. } = result.unwrap();
        assert_eq!(scheme, Secp256k1);
    }

    #[tokio::test]
    async fn test_add_existing_key() {
        let mut mock = MockCommandRunner::new();
        let key_id = "key-123";
        mock.expect_run().returning(move |_, _, _| {
            Ok(json!({
                "keys": [
                    {
                        "key_id": key_id,
                        "public_key": {
                            "Ed25519": UNTAGGED_PUBLIC_KEY
                        }
                    }
                ]
            }))
        });
        let tmp_dir = TempDir::new().unwrap();
        let tmp_keystore = tmp_dir.path().join("external.keystore");
        let mut external = External::new_for_test(Box::new(mock), Some(tmp_keystore));
        external.save().await.unwrap();
        external
            .add_existing("signer".to_string(), key_id.to_string())
            .await
            .unwrap();
        let keys = external.keys;
        let key = keys.get(&SuiAddress::from_str(ADDRESS).expect("Invalid address format"));
        assert!(key.is_some());
    }

    #[tokio::test]
    async fn test_add_keypair_not_supported() {
        let mock = MockCommandRunner::new();
        let mut external = External::new_for_test(Box::new(mock), None);
        let mut crypto_rng = thread_rng();
        let ed25519_keypair = Ed25519KeyPair::generate(&mut crypto_rng);
        let result = external
            .import(None, SuiKeyPair::Ed25519(ed25519_keypair))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_add_existing_key_not_found() {
        let mut mock = MockCommandRunner::new();
        mock.expect_run()
            .returning(|_, _, _| Ok(json!({"keys": []})));
        let mut external = External::new_for_test(Box::new(mock), None);
        let result = external
            .add_existing("signer".to_string(), "missing-key-id".to_string())
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_exec_error_propagation() {
        let mut mock = MockCommandRunner::new();
        mock.expect_run().returning(|_, _, _| Err(anyhow!("fail")));
        let external = External::new_for_test(Box::new(mock), None);
        let result = external.exec("cmd", "method", json!([1, 2, 3])).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_keys_parsing() {
        let mut mock = MockCommandRunner::new();
        mock.expect_run().returning(move |_, _, _| {
            Ok(json!({
                "keys": [
                    {"key_id": "key-1", "public_key": { "Ed25519": UNTAGGED_PUBLIC_KEY }},
                    {"key_id": "key-2", "public_key": { "Ed25519": UNTAGGED_PUBLIC_KEY }}
                ]
            }))
        });
        let external = External::new_for_test(Box::new(mock), None);
        let keys = external
            .signer_available_keys("signer".to_string())
            .await
            .unwrap();
        assert_eq!(keys.len(), 2);
        assert_eq!(keys[0].key_id, "key-1");
        assert_eq!(keys[1].key_id, "key-2");
    }

    #[tokio::test]
    async fn test_remove_key() {
        let tmp_dir = TempDir::new().unwrap();
        let path = tmp_dir.path().join("external.keystore");

        let mut mock = MockCommandRunner::new();
        mock.expect_run()
            .returning(|_, _, _| Ok(json!({"success": true})));
        let mut external = External::new_for_test(Box::new(mock), Some(path));
        let address = SuiAddress::from_str(ADDRESS).unwrap();
        external.keys.insert(
            address,
            StoredKey {
                public_key: PublicKey::decode_base64(PUBLIC_KEY).unwrap(),
                ext_signer: "signer".to_string(),
                key_id: "key-123".to_string(),
            },
        );
        let result = external.remove(address).await;
        assert!(result.is_ok());
        assert!(!external.keys.contains_key(&address));
    }

    #[test]
    fn test_get_keys() {
        let external = load_external_keystore();
        let keys = external.entries();
        assert!(!keys.is_empty());
        assert!(keys.iter().any(|k| k.encode_base64() == PUBLIC_KEY));
    }

    #[test]
    fn test_get_key_fails() {
        let mut mock = MockCommandRunner::new();
        mock.expect_run()
            .returning(|_, _, _| Ok(json!({"success":true})));
        let external = External::new_for_test(Box::new(mock), None);

        let result = external.export(&SuiAddress::from_str(ADDRESS).unwrap());
        assert!(result.is_err())
    }

    #[tokio::test]
    async fn test_sign_hashed() {
        let skp = SuiKeyPair::Ed25519(Ed25519KeyPair::generate(&mut StdRng::from_seed([0; 32])));

        let msg = b"message";
        let public_key = skp.public();
        let address = SuiAddress::from(&public_key);

        let stored_key = StoredKey {
            public_key,
            ext_signer: "signer".to_string(),
            key_id: "id".to_string(),
        };

        let signature = Signature::new_hashed(msg, &skp);

        let mut mock = MockCommandRunner::new();
        mock.expect_run().returning(move |_, _, _| {
            Ok(json!({
                "signature": signature,
            }))
        });
        let mut external = External::new_for_test(Box::new(mock), None);

        external.keys.insert(address, stored_key.clone());

        let message = b"message";
        let result = external.sign_hashed(&address, message).await;
        assert!(result.is_ok());

        // invalid signature rejected
        let mut mock = MockCommandRunner::new();
        mock.expect_run().returning(move |_, _, _| {
            let bytes = vec![0; 97];
            let bad_signature = Signature::from_bytes(&bytes).unwrap();
            Ok(json!({
                "signature": bad_signature,
            }))
        });

        let mut external = External::new_for_test(Box::new(mock), None);
        external.keys.insert(address, stored_key);

        let result = external.sign_hashed(&address, message).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_sign_secure() {
        let skp = SuiKeyPair::Ed25519(Ed25519KeyPair::generate(&mut StdRng::from_seed([0; 32])));

        let msg = b"message";
        let public_key = skp.public();
        let address = SuiAddress::from(&public_key);

        let stored_key = StoredKey {
            public_key,
            ext_signer: "signer".to_string(),
            key_id: "id".to_string(),
        };

        let intent = Intent::sui_transaction();
        let intent_msg = IntentMessage::new(intent.clone(), msg);
        let signature = Signature::new_secure(&intent_msg, &skp);

        let mut mock = MockCommandRunner::new();
        mock.expect_run().returning(move |_, _, _| {
            Ok(json!({
                "signature": signature,
            }))
        });

        let mut external = External::new_for_test(Box::new(mock), None);
        external.keys.insert(address, stored_key.clone());

        let result = external.sign_secure(&address, msg, intent.clone()).await;
        assert!(result.is_ok());

        // invalid signature rejected
        let mut mock = MockCommandRunner::new();
        mock.expect_run().returning(move |_, _, _| {
            let bytes = vec![0; 97];
            let bad_signature = Signature::from_bytes(&bytes).unwrap();
            Ok(json!({
                "signature": bad_signature,
            }))
        });

        let mut external = External::new_for_test(Box::new(mock), None);
        external.keys.insert(address, stored_key);

        let result = external.sign_secure(&address, msg, intent).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_addresses_with_alias() {
        let external = load_external_keystore();
        let addresses_with_alias = external.addresses_with_alias();
        assert!(!addresses_with_alias.is_empty());
        assert!(addresses_with_alias
            .iter()
            .any(|(addr, alias)| { addr.to_string() == ADDRESS && alias.alias == "test_alias" }));
    }

    #[test]
    fn test_aliases() {
        let external = load_external_keystore();
        let aliases = external.aliases();
        assert!(!aliases.is_empty());
        assert!(aliases.iter().any(|a| a.alias == "test_alias"));
    }

    #[test]
    fn test_aliases_mut() {
        let mut external = load_external_keystore();
        let aliases = external.aliases_mut();
        assert!(!aliases.is_empty());
        for alias in aliases {
            alias.alias = "new_alias".to_string();
        }
        assert!(external.aliases().iter().all(|a| a.alias == "new_alias"));
    }

    #[test]
    fn test_get_by_identity() {
        let external = load_external_keystore();
        let address = SuiAddress::from_str(ADDRESS).unwrap();
        let identity = KeyIdentity::Address(address);
        let address = external.get_by_identity(&identity).unwrap();
        assert_eq!(address.to_string(), ADDRESS);
    }

    #[test]
    fn test_create_alias() {
        let mut external = load_external_keystore();
        // clear existing aliases
        external.aliases = BTreeMap::new();

        let alias = external.create_alias(None).unwrap();
        assert!(!alias.is_empty());
        assert!(external.aliases().iter().all(|a| a.alias == alias));

        // clear
        external.aliases.clear();

        let alias = external.create_alias(Some("my_alias".to_string())).unwrap();
        assert_eq!(alias, "my_alias");
        assert!(external.aliases().iter().all(|a| a.alias == alias));
    }

    #[tokio::test]
    async fn test_update_alias() {
        let tmp_dir = TempDir::new().unwrap();
        let path = tmp_dir.path().join("external.keystore");

        let old_alias = "old_alias".to_string();
        let new_alias = "updated_alias".to_string();

        let mut external = External {
            aliases: BTreeMap::default(),
            keys: BTreeMap::default(),
            command_runner: Box::new(StdCommandRunner),
            path: Some(path.clone()),
        };

        external.keys.insert(
            SuiAddress::from_str(ADDRESS).unwrap(),
            StoredKey {
                public_key: PublicKey::decode_base64(PUBLIC_KEY).unwrap(),
                ext_signer: "signer".to_string(),
                key_id: "key-123".to_string(),
            },
        );

        external.aliases.insert(
            SuiAddress::from_str(ADDRESS).unwrap(),
            crate::keystore::Alias {
                alias: old_alias.clone(),
                public_key_base64: PUBLIC_KEY.to_string(),
            },
        );

        assert_ne!(old_alias, new_alias);

        let updated_alias = external
            .update_alias(&old_alias, Some(&new_alias))
            .await
            .unwrap();
        assert_eq!(updated_alias, new_alias);

        // Verify the alias was updated
        assert!(!external.alias_exists(&old_alias));
        assert!(external.alias_exists(&new_alias));
    }
}
