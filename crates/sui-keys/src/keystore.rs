// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub use crate::external::External;
use crate::key_derive::{derive_key_pair_from_path, generate_new_key};
use crate::key_identity::KeyIdentity;
use crate::random_names::{random_name, random_names};

use anyhow::{anyhow, bail, ensure, Context};
use async_trait::async_trait;
use bip32::DerivationPath;
use bip39::{Language, Mnemonic, Seed};
use rand::{rngs::StdRng, SeedableRng};
use regex::Regex;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use shared_crypto::intent::{Intent, IntentMessage};
use std::collections::{BTreeMap, HashSet};
use std::fmt::Write;
use std::fmt::{Display, Formatter};
use std::io::BufReader;
use std::path::{Path, PathBuf};
use sui_types::base_types::SuiAddress;
use sui_types::crypto::get_key_pair_from_rng;
use sui_types::crypto::{
    enum_dispatch, EncodeDecodeBase64, PublicKey, Signature, SignatureScheme, SuiKeyPair,
};

pub const ALIASES_FILE_EXTENSION: &str = "aliases";

#[derive(Serialize, Deserialize)]
#[enum_dispatch(AccountKeystore)]
pub enum Keystore {
    File(FileBasedKeystore),
    InMem(InMemKeystore),
    External(External),
}

pub struct LocalGenerate {
    pub key_scheme: SignatureScheme,
    pub derivation_path: Option<DerivationPath>,
    pub word_length: Option<String>,
}

pub enum GenerateOptions {
    /// Default options for key generation of any keystore type.
    Default,
    /// File or InMem keystore
    Local(LocalGenerate),
    /// Signer
    ExternalSigner(String),
}

impl Default for GenerateOptions {
    fn default() -> Self {
        Self::Default
    }
}

pub struct GeneratedKey {
    pub address: SuiAddress,
    pub public_key: PublicKey,
    pub scheme: SignatureScheme,
}

#[async_trait]
#[enum_dispatch]
pub trait AccountKeystore: Send + Sync {
    /// Generate a new keypair and add it into the keystore.
    async fn generate(
        &mut self,
        alias: Option<String>,
        generate_options: GenerateOptions,
    ) -> Result<GeneratedKey, anyhow::Error> {
        let (key_scheme, derivation_path, word_length) = match generate_options {
            GenerateOptions::Default => (SignatureScheme::ED25519, None, None),
            GenerateOptions::Local(opts) => {
                (opts.key_scheme, opts.derivation_path, opts.word_length)
            }
            GenerateOptions::ExternalSigner(_) => {
                return Err(anyhow!(
                    "Generating new keypair is not supported for this keystore type"
                ));
            }
        };

        let (address, kp, scheme, _phrase) =
            generate_new_key(key_scheme, derivation_path, word_length)?;
        let public_key = kp.public();
        self.import(alias, kp).await?;
        Ok(GeneratedKey {
            address,
            public_key,
            scheme,
        })
    }

    /// Import a keypair into the keystore from a `SuiKeyPair` and optional alias.
    async fn import(
        &mut self,
        alias: Option<String>,
        keypair: SuiKeyPair,
    ) -> Result<(), anyhow::Error>;
    /// Remove a keypair from the keystore by its address.
    async fn remove(&mut self, address: SuiAddress) -> Result<(), anyhow::Error>;
    /// Return an array of `PublicKey`, consisting of every public key in the keystore.
    fn entries(&self) -> Vec<PublicKey>;
    /// Return `SuiKeyPair` for the given address.
    fn export(&self, address: &SuiAddress) -> Result<&SuiKeyPair, anyhow::Error>;

    /// Sign a hash with the keypair corresponding to the given address.
    async fn sign_hashed(
        &self,
        address: &SuiAddress,
        msg: &[u8],
    ) -> Result<Signature, signature::Error>;

    /// Sign a message with the keypair corresponding to the given address with the given intent.
    async fn sign_secure<T>(
        &self,
        address: &SuiAddress,
        msg: &T,
        intent: Intent,
    ) -> Result<Signature, signature::Error>
    where
        T: Serialize + Sync;

    /// Return an array of `SuiAddress`, consisting of every address in the keystore.
    fn addresses(&self) -> Vec<SuiAddress> {
        self.entries().iter().map(|k| k.into()).collect()
    }
    /// Return an array of (&SuiAddress, &Alias), consisting of every address and its corresponding alias in the keystore.
    fn addresses_with_alias(&self) -> Vec<(&SuiAddress, &Alias)>;
    /// Return an array of `Alias`, consisting of every alias stored. Each alias corresponds to a key.
    fn aliases(&self) -> Vec<&Alias>;
    /// Mutable references to each alias in the keystore.
    fn aliases_mut(&mut self) -> Vec<&mut Alias>;
    /// Get the alias of an address.
    fn get_alias(&self, address: &SuiAddress) -> Result<String, anyhow::Error>;
    /// Check if an alias exists by its name
    fn alias_exists(&self, alias: &str) -> bool {
        self.aliases().iter().any(|a| a.alias == alias)
    }

    /// Get address by its identity: a type which is either an address or an alias.
    fn get_by_identity(&self, key_identity: KeyIdentity) -> Result<SuiAddress, anyhow::Error> {
        match key_identity {
            KeyIdentity::Address(addr) => Ok(addr),
            KeyIdentity::Alias(alias) => Ok(*self
                .addresses_with_alias()
                .iter()
                .find(|(_, a)| a.alias == alias)
                .ok_or_else(|| anyhow!("Cannot resolve alias {alias} to an address"))?
                .0),
        }
    }

    /// Returns an alias string. Optional string can be passed, checks for duplicates
    fn create_alias(&self, alias: Option<String>) -> Result<String, anyhow::Error>;

    /// Updates the alias of an existing keypair.
    async fn update_alias(
        &mut self,
        old_alias: &str,
        new_alias: Option<&str>,
    ) -> Result<String, anyhow::Error>;

    // Internal function. Use update_alias instead
    fn update_alias_value(
        &mut self,
        old_alias: &str,
        new_alias: Option<&str>,
    ) -> Result<String, anyhow::Error> {
        if !self.aliases().iter().any(|a| a.alias == old_alias) {
            bail!("The provided alias {old_alias} does not exist");
        }
        let new_alias_name = self.create_alias(new_alias.map(str::to_string))?;
        for a in self.aliases_mut() {
            if a.alias == old_alias {
                let pk = &a.public_key_base64;
                *a = Alias {
                    alias: new_alias_name.clone(),
                    public_key_base64: pk.clone(),
                };
            }
        }
        Ok(new_alias_name)
    }

    /// Import from a mnemonic phrase.
    async fn import_from_mnemonic(
        &mut self,
        phrase: &str,
        key_scheme: SignatureScheme,
        derivation_path: Option<DerivationPath>,
        alias: Option<String>,
    ) -> Result<SuiAddress, anyhow::Error> {
        let mnemonic = Mnemonic::from_phrase(phrase, Language::English)
            .map_err(|e| anyhow::anyhow!("Invalid mnemonic phrase: {:?}", e))?;
        let seed = Seed::new(&mnemonic, "");
        match derive_key_pair_from_path(seed.as_bytes(), derivation_path, &key_scheme) {
            Ok((address, kp)) => {
                self.import(alias, kp).await?;
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
            Keystore::External(_external) => {
                writeln!(writer, "Keystore Type : External")
            }
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Alias {
    pub alias: String,
    pub public_key_base64: String,
}

#[derive(Default)]
pub struct FileBasedKeystore {
    keys: BTreeMap<SuiAddress, SuiKeyPair>,
    aliases: BTreeMap<SuiAddress, Alias>,
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
        FileBasedKeystore::load_or_create(&PathBuf::from(String::deserialize(deserializer)?))
            .map_err(D::Error::custom)
    }
}

#[async_trait]
impl AccountKeystore for FileBasedKeystore {
    async fn sign_hashed(
        &self,
        address: &SuiAddress,
        msg: &[u8],
    ) -> Result<Signature, signature::Error> {
        Ok(Signature::new_hashed(
            msg,
            self.keys.get(address).ok_or_else(|| {
                signature::Error::from_source(format!("Cannot find key for address: [{address}]"))
            })?,
        ))
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
        Ok(Signature::new_secure(
            &IntentMessage::new(intent, msg),
            self.keys.get(address).ok_or_else(|| {
                signature::Error::from_source(format!("Cannot find key for address: [{address}]"))
            })?,
        ))
    }

    async fn import(
        &mut self,
        alias: Option<String>,
        keypair: SuiKeyPair,
    ) -> Result<(), anyhow::Error> {
        let address: SuiAddress = (&keypair.public()).into();
        let alias = self.create_alias(alias)?;
        self.aliases.insert(
            address,
            Alias {
                alias,
                public_key_base64: keypair.public().encode_base64(),
            },
        );
        self.keys.insert(address, keypair);
        self.save().await?;
        Ok(())
    }

    async fn remove(&mut self, address: SuiAddress) -> Result<(), anyhow::Error> {
        self.aliases.remove(&address);
        self.keys.remove(&address);
        self.save().await?;
        Ok(())
    }

    /// Return an array of `Alias`, consisting of every alias and its corresponding public key.
    fn aliases(&self) -> Vec<&Alias> {
        self.aliases.values().collect()
    }

    fn addresses_with_alias(&self) -> Vec<(&SuiAddress, &Alias)> {
        self.aliases.iter().collect::<Vec<_>>()
    }

    /// Return an array of `Alias`, consisting of every alias and its corresponding public key.
    fn aliases_mut(&mut self) -> Vec<&mut Alias> {
        self.aliases.values_mut().collect()
    }

    fn entries(&self) -> Vec<PublicKey> {
        self.keys.values().map(|key| key.public()).collect()
    }

    /// This function returns an error if the provided alias already exists. If the alias
    /// has not already been used, then it returns the alias.
    /// If no alias has been passed, it will generate a new alias.
    fn create_alias(&self, alias: Option<String>) -> Result<String, anyhow::Error> {
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

    /// Get the alias if it exists, or return an error if it does not exist.
    fn get_alias(&self, address: &SuiAddress) -> Result<String, anyhow::Error> {
        match self.aliases.get(address) {
            Some(alias) => Ok(alias.alias.clone()),
            None => bail!("Cannot find alias for address {address}"),
        }
    }

    fn export(&self, address: &SuiAddress) -> Result<&SuiKeyPair, anyhow::Error> {
        match self.keys.get(address) {
            Some(key) => Ok(key),
            None => Err(anyhow!("Cannot find key for address: [{address}]")),
        }
    }

    /// Updates an old alias to the new alias and saves it to the alias file.
    /// If the new_alias is None, it will generate a new random alias.
    async fn update_alias(
        &mut self,
        old_alias: &str,
        new_alias: Option<&str>,
    ) -> Result<String, anyhow::Error> {
        let new_alias_name = self.update_alias_value(old_alias, new_alias)?;
        self.save_aliases().await?;
        Ok(new_alias_name)
    }
}

impl FileBasedKeystore {
    pub fn load_or_create(path: &PathBuf) -> Result<Self, anyhow::Error> {
        let keys = if path.exists() {
            let reader =
                BufReader::new(std::fs::File::open(path).with_context(|| {
                    format!("Cannot open the keystore file: {}", path.display())
                })?);
            let kp_strings: Vec<String> = serde_json::from_reader(reader).with_context(|| {
                format!("Cannot deserialize the keystore file: {}", path.display(),)
            })?;
            kp_strings
                .iter()
                .map(|kpstr| {
                    let key = SuiKeyPair::decode_base64(kpstr);
                    key.map(|k| (SuiAddress::from(&k.public()), k))
                })
                .collect::<Result<BTreeMap<_, _>, _>>()
                .map_err(|e| anyhow!("Invalid keystore file: {}. {}", path.display(), e))?
        } else {
            BTreeMap::new()
        };

        // check aliases
        let mut aliases_path = path.clone();
        aliases_path.set_extension(ALIASES_FILE_EXTENSION);

        let aliases = if aliases_path.exists() {
            let reader = BufReader::new(std::fs::File::open(&aliases_path).with_context(|| {
                format!(
                    "Cannot open aliases file in keystore: {}",
                    aliases_path.display()
                )
            })?);

            let aliases: Vec<Alias> = serde_json::from_reader(reader).with_context(|| {
                format!(
                    "Cannot deserialize aliases file in keystore: {}",
                    aliases_path.display(),
                )
            })?;

            aliases
                .into_iter()
                .map(|alias| {
                    let key = PublicKey::decode_base64(&alias.public_key_base64);
                    key.map(|k| (Into::<SuiAddress>::into(&k), alias))
                })
                .collect::<Result<BTreeMap<_, _>, _>>()
                .map_err(|e| {
                    anyhow!(
                        "Invalid aliases file in keystore: {}. {}",
                        aliases_path.display(),
                        e
                    )
                })?
        } else if keys.is_empty() {
            BTreeMap::new()
        } else {
            let names: Vec<String> = random_names(HashSet::new(), keys.len());
            let aliases = keys
                .iter()
                .zip(names)
                .map(|((sui_address, skp), alias)| {
                    let public_key_base64 = skp.public().encode_base64();
                    (
                        *sui_address,
                        Alias {
                            alias,
                            public_key_base64,
                        },
                    )
                })
                .collect::<BTreeMap<_, _>>();
            let aliases_store = serde_json::to_string_pretty(&aliases.values().collect::<Vec<_>>())
                .with_context(|| {
                    format!(
                        "Cannot serialize aliases to file in keystore: {}",
                        aliases_path.display()
                    )
                })?;

            std::fs::write(aliases_path, aliases_store)?;
            aliases
        };

        Ok(Self {
            keys,
            aliases,
            path: Some(path.to_path_buf()),
        })
    }

    pub fn set_path(&mut self, path: &Path) {
        self.path = Some(path.to_path_buf());
    }

    pub async fn save_aliases(&self) -> Result<(), anyhow::Error> {
        if let Some(path) = &self.path {
            let aliases_store =
                serde_json::to_string_pretty(&self.aliases.values().collect::<Vec<_>>())
                    .with_context(|| {
                        format!(
                            "Cannot serialize aliases to file in keystore: {}",
                            path.display()
                        )
                    })?;

            let mut aliases_path = path.clone();
            aliases_path.set_extension(ALIASES_FILE_EXTENSION);
            // no reactor for tokio::fs::write in simtest, so we use spawn_blocking
            tokio::task::spawn_blocking(move || std::fs::write(aliases_path, aliases_store))
                .await?
                .with_context(|| format!("Cannot write aliases to file: {}", path.display()))?;
        }
        Ok(())
    }

    /// Keys saved as Base64 with 33 bytes `flag || privkey` ($BASE64_STR).
    /// To see Bech32 format encoding, use `sui keytool export $SUI_ADDRESS` where
    /// $SUI_ADDRESS can be found with `sui keytool list`. Or use `sui keytool convert $BASE64_STR`
    pub async fn save_keystore(&self) -> Result<(), anyhow::Error> {
        if let Some(path) = &self.path {
            let store = serde_json::to_string_pretty(
                &self
                    .keys
                    .values()
                    .map(|k| k.encode_base64())
                    .collect::<Vec<_>>(),
            )
            .with_context(|| format!("Cannot serialize keystore to file: {}", path.display()))?;
            let keystore_path = path.clone();
            // no reactor for tokio::fs::write in simtest, so we use spawn_blocking
            tokio::task::spawn_blocking(move || std::fs::write(keystore_path, store))
                .await?
                .with_context(|| format!("Cannot write keystore to file: {}", path.display()))?;
        }
        Ok(())
    }

    pub async fn save(&self) -> Result<(), anyhow::Error> {
        self.save_aliases().await?;
        self.save_keystore().await?;
        Ok(())
    }

    pub fn key_pairs(&self) -> Vec<&SuiKeyPair> {
        self.keys.values().collect()
    }
}

#[derive(Default, Serialize, Deserialize)]
pub struct InMemKeystore {
    aliases: BTreeMap<SuiAddress, Alias>,
    keys: BTreeMap<SuiAddress, SuiKeyPair>,
}

#[async_trait]
impl AccountKeystore for InMemKeystore {
    async fn sign_hashed(
        &self,
        address: &SuiAddress,
        msg: &[u8],
    ) -> Result<Signature, signature::Error> {
        Ok(Signature::new_hashed(
            msg,
            self.keys.get(address).ok_or_else(|| {
                signature::Error::from_source(format!("Cannot find key for address: [{address}]"))
            })?,
        ))
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
        Ok(Signature::new_secure(
            &IntentMessage::new(intent, msg),
            self.keys.get(address).ok_or_else(|| {
                signature::Error::from_source(format!("Cannot find key for address: [{address}]"))
            })?,
        ))
    }

    async fn import(
        &mut self,
        alias: Option<String>,
        keypair: SuiKeyPair,
    ) -> Result<(), anyhow::Error> {
        let address: SuiAddress = (&keypair.public()).into();
        let alias = alias.unwrap_or_else(|| {
            random_name(
                &self
                    .aliases()
                    .iter()
                    .map(|x| x.alias.clone())
                    .collect::<HashSet<_>>(),
            )
        });

        let public_key_base64 = keypair.public().encode_base64();
        let alias = Alias {
            alias,
            public_key_base64,
        };
        self.aliases.insert(address, alias);
        self.keys.insert(address, keypair);
        Ok(())
    }

    async fn remove(&mut self, address: SuiAddress) -> Result<(), anyhow::Error> {
        self.aliases.remove(&address);
        self.keys.remove(&address);
        Ok(())
    }

    /// Get all aliases objects
    fn aliases(&self) -> Vec<&Alias> {
        self.aliases.values().collect()
    }

    fn entries(&self) -> Vec<PublicKey> {
        self.keys.values().map(|key| key.public()).collect()
    }

    fn addresses_with_alias(&self) -> Vec<(&SuiAddress, &Alias)> {
        self.aliases.iter().collect::<Vec<_>>()
    }

    fn export(&self, address: &SuiAddress) -> Result<&SuiKeyPair, anyhow::Error> {
        match self.keys.get(address) {
            Some(key) => Ok(key),
            None => Err(anyhow!("Cannot find key for address: [{address}]")),
        }
    }

    /// Get alias of address
    fn get_alias(&self, address: &SuiAddress) -> Result<String, anyhow::Error> {
        match self.aliases.get(address) {
            Some(alias) => Ok(alias.alias.clone()),
            None => bail!("Cannot find alias for address {address}"),
        }
    }

    /// This function returns an error if the provided alias already exists. If the alias
    /// has not already been used, then it returns the alias.
    /// If no alias has been passed, it will generate a new alias.
    fn create_alias(&self, alias: Option<String>) -> Result<String, anyhow::Error> {
        match alias {
            Some(a) if self.alias_exists(&a) => {
                bail!("Alias {a} already exists. Please choose another alias.")
            }
            Some(a) => validate_alias(&a),
            None => Ok(random_name(
                &self
                    .aliases()
                    .iter()
                    .map(|x| x.alias.to_string())
                    .collect::<HashSet<_>>(),
            )),
        }
    }

    fn aliases_mut(&mut self) -> Vec<&mut Alias> {
        self.aliases.values_mut().collect()
    }

    /// Updates an old alias to the new alias. If the new_alias is None,
    /// it will generate a new random alias.
    async fn update_alias(
        &mut self,
        old_alias: &str,
        new_alias: Option<&str>,
    ) -> Result<String, anyhow::Error> {
        self.update_alias_value(old_alias, new_alias)
    }
}

impl InMemKeystore {
    pub fn new_insecure_for_tests(initial_key_number: usize) -> Self {
        let mut rng = StdRng::from_seed([0; 32]);
        let keys = (0..initial_key_number)
            .map(|_| get_key_pair_from_rng(&mut rng))
            .map(|(ad, k)| (ad, SuiKeyPair::Ed25519(k)))
            .collect::<BTreeMap<SuiAddress, SuiKeyPair>>();

        let aliases = keys
            .iter()
            .zip(random_names(HashSet::new(), keys.len()))
            .map(|((sui_address, skp), alias)| {
                let public_key_base64 = skp.public().encode_base64();
                (
                    *sui_address,
                    Alias {
                        alias,
                        public_key_base64,
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();

        Self { aliases, keys }
    }
}

pub(crate) fn validate_alias(alias: &str) -> Result<String, anyhow::Error> {
    let re = Regex::new(r"^[A-Za-z][A-Za-z0-9-_\.]*$")
        .map_err(|_| anyhow!("Cannot build the regex needed to validate the alias naming"))?;
    let alias = alias.trim();
    ensure!(
        re.is_match(alias),
        "Invalid alias. A valid alias must start with a letter and can contain only letters, digits, hyphens (-), dots (.), or underscores (_)."
    );
    Ok(alias.to_string())
}

#[cfg(test)]
mod tests {
    use crate::keystore::validate_alias;

    #[test]
    fn validate_alias_test() {
        // OK
        assert!(validate_alias("A.B_dash").is_ok());
        assert!(validate_alias("A.B-C1_dash").is_ok());
        assert!(validate_alias("abc_123.sui").is_ok());
        // Not allowed
        assert!(validate_alias("A.B-C_dash!").is_err());
        assert!(validate_alias(".B-C_dash!").is_err());
        assert!(validate_alias("_test").is_err());
        assert!(validate_alias("123").is_err());
        assert!(validate_alias("@@123").is_err());
        assert!(validate_alias("@_Ab").is_err());
        assert!(validate_alias("_Ab").is_err());
        assert!(validate_alias("^A").is_err());
        assert!(validate_alias("-A").is_err());
    }
}
