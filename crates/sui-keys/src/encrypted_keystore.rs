// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::key_derive::{derive_key_pair_from_path, generate_new_key};
use crate::random_names::{random_name, random_names};
use anyhow::{anyhow, bail, ensure, Context};
use bip32::DerivationPath;
use bip39::{Language, Mnemonic, Seed};
use rand::{rngs::StdRng, SeedableRng, RngCore};
use rand::rngs::OsRng;
use regex::Regex;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use shared_crypto::intent::{Intent, IntentMessage};
use std::collections::{BTreeMap, HashSet};
use std::fmt::Write as FmtWrite;
use std::fmt::{Display, Formatter};
use std::fs;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::get_key_pair_from_rng;
use sui_types::crypto::{
    enum_dispatch, EncodeDecodeBase64, PublicKey, Signature, SignatureScheme, SuiKeyPair,
};
use std::num::NonZeroU32;
use ring::aead::{self, Aad, LessSafeKey, Nonce, UnboundKey, NONCE_LEN};
use ring::pbkdf2;
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use zeroize::Zeroize;

// Encryption-related constants
const PBKDF2_ITERATIONS: u32 = 100_000;
const SALT_SIZE: usize = 16;
const KEY_SIZE: usize = 32;

/// Structure defining the file format of the encrypted keystore
#[derive(Serialize, Deserialize)]
pub struct EncryptedKeyData {
    /// Encryption version (for future version upgrades)
    pub version: u8,
    /// IV used for encryption (Base64 encoded)
    pub iv: String,
    /// Salt used for key derivation from password (Base64 encoded)
    pub salt: String,
    /// The encryption cipher used
    pub cipher: String,
    /// Number of iterations for PBKDF2
    pub iterations: u32,
    /// The Sui address corresponding to this key
    pub address: String,
    /// The public key
    pub public_key: String,
    /// The ciphertext (Base64 encoded)
    pub ciphertext: String,
}

/// Decrypt keypair from EncryptedKeyData
/// This function securely decrypts the keypair using the provided password.
/// It uses PBKDF2 for key derivation and AES-256-GCM for decryption.
/// Sensitive data is zeroed from memory after use.
pub fn decrypt_key_pair(encrypted_data: &EncryptedKeyData, password: &str) -> Result<SuiKeyPair, anyhow::Error> {
    // Prepare decryption parameter
    if encrypted_data.cipher != "aes-256-gcm" {
        return Err(anyhow!("Unsupported cipher: {}", encrypted_data.cipher));
    }
    
    let ciphertext = BASE64.decode(&encrypted_data.ciphertext)
        .map_err(|e| anyhow!("Failed to decode ciphertext: {}", e))?;
    
    let iv = BASE64.decode(&encrypted_data.iv)
        .map_err(|e| anyhow!("Failed to decode IV: {}", e))?;
    
    let salt = BASE64.decode(&encrypted_data.salt)
        .map_err(|e| anyhow!("Failed to decode salt: {}", e))?;
    
    if iv.len() != NONCE_LEN {
        return Err(anyhow!("Invalid IV length"));
    }
    
    // Derive key from password
    let iterations = NonZeroU32::new(encrypted_data.iterations)
        .ok_or_else(|| anyhow!("Invalid iteration count"))?;
    
    let mut derived_key = [0u8; KEY_SIZE];
    pbkdf2::derive(
        pbkdf2::PBKDF2_HMAC_SHA256, 
        iterations, 
        &salt, 
        password.as_bytes(), 
        &mut derived_key
    );
    
    // Create a scope to ensure sensitive memory is zeroed
    let result = {
        // Decrypt
        let unbound_key = UnboundKey::new(&aead::AES_256_GCM, &derived_key)
            .map_err(|_| anyhow!("Failed to create decryption key"))?;
        let less_safe_key = LessSafeKey::new(unbound_key);
        
        let mut nonce_bytes = [0u8; NONCE_LEN];
        nonce_bytes.copy_from_slice(&iv);
        let nonce = Nonce::assume_unique_for_key(nonce_bytes);
        let aad = Aad::empty();
        
        let mut ciphertext_copy = ciphertext.clone();
        let plaintext = less_safe_key
            .open_in_place(nonce, aad, &mut ciphertext_copy)
            .map_err(|_| anyhow!("Decryption failed - wrong password or corrupted data"))?;
        
        // Restore keypair
        let plaintext_str = std::str::from_utf8(plaintext)
            .map_err(|e| anyhow!("Failed to convert key data: {}", e))?;
        
        SuiKeyPair::decode_base64(plaintext_str)
            .map_err(|e| anyhow!("Failed to restore keypair: {}", e))
    };
    
    derived_key.zeroize();
    
    result
}

/// Verify keypair address matches address in encrypted data
/// This is a critical security check to ensure the decrypted key corresponds
/// to the expected address stored in the encrypted file, protecting against
/// potential tampering or corruption of the key file.
pub fn verify_key_address(keypair: &SuiKeyPair, encrypted_data: &EncryptedKeyData) -> Result<SuiAddress, anyhow::Error> {
    let address_from_keypair: SuiAddress = (&keypair.public()).into();
    let expected_address: SuiAddress = SuiAddress::from_str(&encrypted_data.address)?;
    
    if address_from_keypair != expected_address {
        return Err(anyhow!(
            "Address mismatch: file stored address {}, actual key address {}", 
            expected_address, address_from_keypair
        ));
    }
    
    Ok(address_from_keypair)
}

/// Sign transaction data with encrypted key data
/// This function securely signs transaction data using password-protected encrypted key.
/// The key is temporarily decrypted in memory and immediately zeroed after signing.
pub fn sign_encrypted<T>(
    key_data: &EncryptedKeyData,
    password: &str,
    msg: &T,
    intent: Intent,
) -> Result<Signature, anyhow::Error> 
where
    T: Serialize,
{
    // Decrypt the key and verify the address
    let keypair = decrypt_key_pair(key_data, password)?;
    verify_key_address(&keypair, key_data)?;
    
    Ok(Signature::new_secure(
        &IntentMessage::new(intent, msg),
        &keypair,
    ))
}

/// Create a new encrypted key file
/// Encrypts the provided keypair with the given password using AES-256-GCM
/// and writes it to a file named with the SUI address.
pub fn create_encrypted_key(
    password: &str,
    keypair: &SuiKeyPair
) -> Result<EncryptedKeyData, anyhow::Error> {    
    // Generate encryption parameters
    let mut salt = [0u8; SALT_SIZE];
    let mut rng = OsRng;
    rng.fill_bytes(&mut salt);
    
    let mut iv = [0u8; NONCE_LEN];
    rng.fill_bytes(&mut iv);
    
    // Encrypt and save
    let encrypted_data = create_encrypt_key_data(keypair, password, &salt, &iv)?;
    
    Ok(encrypted_data)
}

/// Internal function to encrypt and save keypair to file
fn create_encrypt_key_data(
    keypair: &SuiKeyPair,
    password: &str,
    salt: &[u8],
    iv: &[u8; NONCE_LEN]
) -> Result<EncryptedKeyData, anyhow::Error> {
    // Derive encryption key from password
    let iterations = NonZeroU32::new(PBKDF2_ITERATIONS).unwrap();
    
    let mut derived_key = [0u8; KEY_SIZE]; // AES-256 key size
    pbkdf2::derive(
        pbkdf2::PBKDF2_HMAC_SHA256, 
        iterations, 
        salt, 
        password.as_bytes(), 
        &mut derived_key
    );
    
    // Serialize key pair
    let serialized_keypair = keypair.encode_base64();
    
    // Encrypt
    let unbound_key = UnboundKey::new(&aead::AES_256_GCM, &derived_key)
        .map_err(|_| anyhow!("Failed to create encryption key"))?;
    let less_safe_key = LessSafeKey::new(unbound_key);
    
    let nonce = Nonce::assume_unique_for_key(*iv);
    let aad = Aad::empty(); // No additional authentication data
    
    let mut serialized_data = serialized_keypair.as_bytes().to_vec();
    less_safe_key
        .seal_in_place_append_tag(nonce, aad, &mut serialized_data)
        .map_err(|_| anyhow!("Encryption failed"))?;
    
    // Address for verification
    let address: SuiAddress = (&keypair.public()).into();
    
    // Create encrypted data structure
    let encrypted_data = EncryptedKeyData {
        version: 1,
        iv: BASE64.encode(iv),
        salt: BASE64.encode(salt),
        cipher: "aes-256-gcm".to_string(),
        iterations: PBKDF2_ITERATIONS,
        address: address.to_string(),
        public_key: keypair.public().encode_base64(),
        ciphertext: BASE64.encode(&serialized_data),
    };
    
    derived_key.zeroize();
    let mut cleared_string = serialized_keypair;
    cleared_string.zeroize();
    serialized_data.zeroize();
    
    Ok(encrypted_data)
}

#[derive(Serialize, Deserialize)]
#[enum_dispatch(AccountEncryptedKeystore)]
pub enum EncryptedKeystore {
    File(EncryptedFileBasedKeystore),
}

#[enum_dispatch]
pub trait AccountEncryptedKeystore: Send + Sync {
    fn add_key(&mut self, alias: Option<String>, encrypted_key: EncryptedKeyData) -> Result<(), anyhow::Error>;
    fn keys(&self) -> Vec<PublicKey>;
    fn get_key(&self, address: &SuiAddress) -> Result<&EncryptedKeyData, anyhow::Error>;

    fn sign_hashed(&self, address: &SuiAddress, msg: &[u8]) -> Result<Signature, signature::Error>;

    fn sign_secure<T>(
        &self,
        address: &SuiAddress,
        msg: &T,
        intent: Intent,
    ) -> Result<Signature, signature::Error>
    where
        T: Serialize;
    fn addresses(&self) -> Vec<SuiAddress> {
        let mut addresses: Vec<SuiAddress> = self.keys().iter().map(|k| k.into()).collect();
        addresses.extend(self.encrypted_addresses());        
        addresses
    }
    fn addresses_with_alias(&self) -> Vec<(&SuiAddress, &Alias)>;
    fn aliases(&self) -> Vec<&Alias>;
    fn aliases_mut(&mut self) -> Vec<&mut Alias>;
    fn alias_names(&self) -> Vec<&str> {
        self.aliases()
            .into_iter()
            .map(|a| a.alias.as_str())
            .collect()
    }
    /// Get alias of address
    fn get_alias_by_address(&self, address: &SuiAddress) -> Result<String, anyhow::Error>;
    fn get_address_by_alias(&self, alias: String) -> Result<&SuiAddress, anyhow::Error>;
    /// Check if an alias exists by its name
    fn alias_exists(&self, alias: &str) -> bool {
        self.alias_names().contains(&alias)
    }

    fn create_alias(&self, alias: Option<String>) -> Result<String, anyhow::Error>;

    fn update_alias(
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
        if !self.alias_exists(old_alias) {
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

    fn generate_and_add_new_key(
        &mut self,
        key_scheme: SignatureScheme,
        alias: Option<String>,
        derivation_path: Option<DerivationPath>,
        word_length: Option<String>,
    ) -> Result<(SuiAddress, String, SignatureScheme), anyhow::Error> {
        let (address, kp, scheme, phrase) =
            generate_new_key(key_scheme, derivation_path, word_length)?;

        let password = prompt_password("Enter password: ")
            .map_err(|e| anyhow!(e.to_string()))?;
        let _confirm_pwd = prompt_password("Confirm password: ")
            .map_err(|e| anyhow!(e.to_string()))?;
        
        if password != _confirm_pwd {
            return Err(anyhow!("Passwords do not match"));
        }

        let encrypted_data = create_encrypted_key(&password, &kp)?;
        self.add_key(alias, encrypted_data)?;
    
        Ok((address, phrase, scheme))
    }

    fn import_from_mnemonic(
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
                let password = prompt_password("Enter password: ")
                .map_err(|e| anyhow!(e.to_string()))?;
                let _confirm_pwd = prompt_password("Confirm password: ")
                    .map_err(|e| anyhow!(e.to_string()))?;
                
                if password != _confirm_pwd {
                    return Err(anyhow!("Passwords do not match"));
                }

                let encrypted_data = create_encrypted_key(&password, &kp)?;
                self.add_key(alias,  encrypted_data)?;
                Ok(address)
            }
            Err(e) => Err(anyhow!("error getting keypair {:?}", e)),
        }
    }

    fn encrypted_addresses(&self) -> Vec<SuiAddress> {
        vec![] 
    }
}

impl Display for EncryptedKeystore {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut writer = String::new();
        match self {
            EncryptedKeystore::File(file) => {
                writeln!(writer, "Keystore Type : File")?;
                write!(writer, "Keystore Path : {:?}", file.path)?;
                write!(f, "{}", writer)
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
pub struct EncryptedFileBasedKeystore {
    keys: BTreeMap<SuiAddress, EncryptedKeyData>,
    aliases: BTreeMap<SuiAddress, Alias>,
    path: Option<PathBuf>,
}

impl Serialize for EncryptedFileBasedKeystore {
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

impl<'de> Deserialize<'de> for EncryptedFileBasedKeystore {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::Error;
        EncryptedFileBasedKeystore::new(&PathBuf::from(String::deserialize(deserializer)?))
            .map_err(D::Error::custom)
    }
}

/// Helper function to prompt for password and securely handle it
/// Centralizes password input logic to avoid code duplication
fn prompt_password(prompt: &str) -> Result<String, signature::Error> {
    rpassword::prompt_password(prompt)
        .map_err(|e| signature::Error::from_source(e.to_string()))
}

impl AccountEncryptedKeystore for EncryptedFileBasedKeystore {
    fn sign_hashed(&self, address: &SuiAddress, msg: &[u8]) -> Result<Signature, signature::Error> {
        if let Some(encrypted_key) = self.keys.get(address) {
            let password = prompt_password("Enter password for key file: ")?;
            
            let keypair = decrypt_key_pair(encrypted_key, &password)
                .map_err(|e| signature::Error::from_source(e.to_string()))?;
            
            Ok(Signature::new_hashed(msg, &keypair))
        } else {
            Err(signature::Error::from_source(format!("Cannot find key for address: [{address}]")))
        }
    }
    
    fn sign_secure<T>(
        &self,
        address: &SuiAddress,
        msg: &T,
        intent: Intent,
    ) -> Result<Signature, signature::Error>
    where
        T: Serialize,
    {
        if let Some(encrypted_key) = self.keys.get(address) {
            let password = prompt_password("Enter password for key file: ")?;
            
            return sign_encrypted(encrypted_key, &password, msg, intent)
                .map_err(|e| signature::Error::from_source(e.to_string()));
        }
        
        Err(signature::Error::from_source(format!("Cannot find key for address: [{address}]")))
    }

    fn add_key(&mut self, alias: Option<String>, encrypted_key: EncryptedKeyData) -> Result<(), anyhow::Error> {
        let address: SuiAddress = SuiAddress::from_str(&encrypted_key.address)
            .map_err(|e| anyhow!("Invalid address in encrypted key: {}", e))?;
        let alias = self.create_alias(alias)?;
        let public_key = encrypted_key.public_key.clone();

        self.aliases.insert(
            address,
            Alias {
                alias,
                public_key_base64: public_key,
            },
        );
        self.keys.insert(address, encrypted_key);
        self.save()?;
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

    fn keys(&self) -> Vec<PublicKey> {
        self.keys.values()
            .filter_map(|key| PublicKey::decode_base64(&key.public_key).ok())
            .collect()
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
                    .alias_names()
                    .into_iter()
                    .map(|x| x.to_string())
                    .collect::<HashSet<_>>(),
            )),
        }
    }

    /// Get the address by its alias
    fn get_address_by_alias(&self, alias: String) -> Result<&SuiAddress, anyhow::Error> {
        self.addresses_with_alias()
            .iter()
            .find(|x| x.1.alias == alias)
            .ok_or_else(|| anyhow!("Cannot resolve alias {alias} to an address"))
            .map(|x| x.0)
    }

    /// Get the alias if it exists, or return an error if it does not exist.
    fn get_alias_by_address(&self, address: &SuiAddress) -> Result<String, anyhow::Error> {
        match self.aliases.get(address) {
            Some(alias) => Ok(alias.alias.clone()),
            None => bail!("Cannot find alias for address {address}"),
        }
    }


    fn get_key(&self, address: &SuiAddress) -> Result<&EncryptedKeyData, anyhow::Error> {
        match self.keys.get(address) {
            Some(key) => Ok(key),
            None => Err(anyhow!("Cannot find key for address: [{address}]")),
        }
    }


    /// Updates an old alias to the new alias and saves it to the alias file.
    /// If the new_alias is None, it will generate a new random alias.
    fn update_alias(
        &mut self,
        old_alias: &str,
        new_alias: Option<&str>,
    ) -> Result<String, anyhow::Error> {
        let new_alias_name = self.update_alias_value(old_alias, new_alias)?;
        self.save_aliases()?;
        Ok(new_alias_name)
    }

    fn encrypted_addresses(&self) -> Vec<SuiAddress> {
        self.keys.keys().cloned().collect()
    }
}

impl EncryptedFileBasedKeystore {
    pub fn new(path: &PathBuf) -> Result<Self, anyhow::Error> {
        let mut keys: BTreeMap<SuiAddress, EncryptedKeyData> = BTreeMap::new();
        
        if path.exists() {
            let reader = BufReader::new(File::open(path).with_context(|| {
                format!("Cannot open the keystore file: {}", path.display())
            })?);
            
            let json_values: serde_json::Value = serde_json::from_reader(reader).with_context(|| {
                format!("Cannot parse the keystore file: {}", path.display())
            })?;
            
            if let serde_json::Value::Array(items) = json_values {
                for item in items {
                    if let serde_json::Value::Object(_) = item {
                        let encrypted_key: EncryptedKeyData = serde_json::from_value(item)
                            .map_err(|e| anyhow!("Invalid encrypted key format: {}", e))?;
                        let address = SuiAddress::from_str(&encrypted_key.address)
                            .map_err(|e| anyhow!("Invalid address in encrypted key: {}", e))?;
                        keys.insert(address, encrypted_key);
                    } else {
                        return Err(anyhow!("Unexpected item in keystore file"));
                    }
                }
            } else {
                return Err(anyhow!("Keystore file is not an array"));
            }
        }

        // check aliases
        let mut aliases_path = path.clone();
        aliases_path.set_extension("aliases");

        let aliases = if aliases_path.exists() {
            let reader = BufReader::new(File::open(&aliases_path).with_context(|| {
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
                .keys()
                .zip(names)
                .map(|(sui_address, alias)| {
                    let key_data = &keys[sui_address];
                    (
                        *sui_address,
                        Alias {
                            alias,
                            public_key_base64: key_data.public_key.clone(),
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
            fs::write(aliases_path, aliases_store)?;
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

    pub fn save_aliases(&self) -> Result<(), anyhow::Error> {
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
            aliases_path.set_extension("aliases");
            fs::write(aliases_path, aliases_store)?
        }
        Ok(())
    }

    /// Keys saved as encrypted data JSON format
    pub fn save_keystore(&self) -> Result<(), anyhow::Error> {
        if let Some(path) = &self.path {
            let mut all_items = Vec::new();
            
            for encrypted_key in self.keys.values() {
                all_items.push(serde_json::to_value(encrypted_key)?);
            }
            
            let store = serde_json::to_string_pretty(&all_items)
                .with_context(|| format!("Cannot serialize keystore to file: {}", path.display()))?;
            fs::write(path, store)?;
        }
        Ok(())
    }

    pub fn save(&self) -> Result<(), anyhow::Error> {
        self.save_aliases()?;
        self.save_keystore()?;
        Ok(())
    }

    pub fn new_insecure_for_tests(initial_key_number: usize) -> Self {
        let mut rng = StdRng::from_seed([0; 32]);
        let mut keys = BTreeMap::new();
        let mut aliases = BTreeMap::new();

        for _ in 0..initial_key_number {
            let (address, kp) = get_key_pair_from_rng(&mut rng);
            let sui_key_pair = SuiKeyPair::Ed25519(kp);
            
            // Use fixed password for testing
            let test_password = "test_password";
            let encrypted_data = create_encrypted_key(test_password, &sui_key_pair)
                .expect("Failed to create encrypted key");
            
            // Generate random name
            let alias = random_name(&HashSet::new());
            
            aliases.insert(
                address,
                Alias {
                    alias: alias.clone(),
                    public_key_base64: sui_key_pair.public().encode_base64(),
                },
            );
            
            keys.insert(address, encrypted_data);
        }

        Self {
            keys,
            aliases,
            path: None,
        }
    }
}

fn validate_alias(alias: &str) -> Result<String, anyhow::Error> {
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
    use crate::encrypted_keystore::validate_alias;

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