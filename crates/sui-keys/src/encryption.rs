// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Password-based encryption of the private keys in a [`FileBasedKeystore`] file.
//!
//! An encrypted keystore file is a JSON object. It keeps the public keys in plaintext, so that
//! commands that only need addresses work without the password. The private keys are the contents
//! of a plaintext keystore file, encrypted with AES-256-GCM under a key that Argon2id derives from
//! the Unicode NFC form of the password. The encryption authenticates every other field of the
//! file except the nonce, so a change to the format version, the KDF parameters or the public keys
//! makes decryption fail. The public keys are therefore authenticated only when the keystore is
//! unlocked: without the password, nothing can tell a public key that someone replaced in the file
//! from a real one.
//!
//! [`FileBasedKeystore`]: crate::keystore::FileBasedKeystore

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::OnceLock;

use anyhow::{anyhow, bail, ensure};
use argon2::{Algorithm, Argon2, Block, Params, Version};
use fastcrypto::aes::{Aes256Gcm, AesKey, AuthenticatedCipher, InitializationVector};
use fastcrypto::encoding::{Base64, Encoding};
use fastcrypto::traits::ToFromBytes;
use once_cell::sync::OnceCell;
use rand::RngCore;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use sui_types::base_types::SuiAddress;
use sui_types::crypto::{EncodeDecodeBase64, PublicKey, SuiKeyPair};
use typenum::U12;
use unicode_normalization::{UnicodeNormalization, is_nfc};
use zeroize::{Zeroize, Zeroizing};

use crate::keystore::{decode_key_pairs, encode_key_pairs};

#[cfg(test)]
#[path = "unit_tests/encryption_tests.rs"]
mod encryption_tests;

/// Environment variable that holds the password of an encrypted keystore. Non-interactive
/// sessions, such as CI jobs, use it to unlock the keystore.
pub const KEYSTORE_PASSWORD_ENV_VAR: &str = "SUI_KEYSTORE_PASSWORD";

/// Environment variable from which `sui keytool change-password` reads the new password.
pub const NEW_KEYSTORE_PASSWORD_ENV_VAR: &str = "SUI_KEYSTORE_NEW_PASSWORD";

/// The number of times the password prompt asks for the password before unlocking fails.
const MAX_PROMPT_ATTEMPTS: usize = 3;

const FORMAT_VERSION: u32 = 1;
const ASSOCIATED_DATA_DOMAIN: &str = "sui-keystore";
const KDF_ALGORITHM: &str = "argon2id";
const CIPHER_ALGORITHM: &str = "aes-256-gcm";
const SALT_LENGTH: usize = 16;
const NONCE_LENGTH: usize = 12;
const TAG_LENGTH: usize = 16;
const KEY_LENGTH: usize = 32;

// The largest Argon2 costs that a keystore file can request: 1 GiB of memory, 16 passes and 16
// lanes, which are 16 times the recommended memory and about 5 times the recommended passes. The
// KDF parameters are authenticated only after the key derivation, so these limits bound the time
// and memory that a damaged or edited file can make an unlock use.
const MAX_MEMORY_KIB: u32 = 1024 * 1024;
const MAX_ITERATIONS: u32 = 16;
const MAX_PARALLELISM: u32 = 16;

/// Function that asks the user for a password, given the message to show. It returns `Ok(None)`
/// if it cannot ask, for example because the process does not run in a terminal.
pub type PasswordPrompt = fn(&str) -> anyhow::Result<Option<String>>;

/// The prompt that [`unlock_with`] uses, which tests replace with a closure.
type PromptFn<'a> = dyn Fn(&str) -> anyhow::Result<Option<String>> + 'a;

static PASSWORD_PROMPT: OnceLock<PasswordPrompt> = OnceLock::new();

/// Sets the function that asks for the password when an operation needs the private keys of a
/// locked keystore and [`KEYSTORE_PASSWORD_ENV_VAR`] is not set. Without a prompt, such an
/// operation fails. Only the first call has an effect.
pub fn set_password_prompt(prompt: PasswordPrompt) {
    let _ = PASSWORD_PROMPT.set(prompt);
}

/// The password environment variables as [`remove_password_env_vars`] read them, each with its
/// value or the error that reading it gave.
type RemovedEnvVars = [(&'static str, Result<Option<Zeroizing<String>>, String>); 2];

static REMOVED_PASSWORD_ENV_VARS: OnceLock<RemovedEnvVars> = OnceLock::new();

/// Moves [`KEYSTORE_PASSWORD_ENV_VAR`] and [`NEW_KEYSTORE_PASSWORD_ENV_VAR`] out of the
/// environment of this process, so that the processes that it starts, such as `git`, do not
/// receive the passwords. [`password_from_env`] still returns their values. Only the first call
/// has an effect.
///
/// # Safety
///
/// As for [`std::env::remove_var`], no other thread may read or write the environment while this
/// function runs. A binary calls it at the start of `main`, before it starts other threads.
pub unsafe fn remove_password_env_vars() {
    if REMOVED_PASSWORD_ENV_VARS.get().is_some() {
        return;
    }
    let removed = [KEYSTORE_PASSWORD_ENV_VAR, NEW_KEYSTORE_PASSWORD_ENV_VAR].map(|name| {
        let value = read_password_env_var(name).map_err(|e| e.to_string());
        // SAFETY: the caller ensures that no other thread uses the environment.
        unsafe { std::env::remove_var(name) };
        (name, value)
    });
    let _ = REMOVED_PASSWORD_ENV_VARS.set(removed);
}

/// Reads a password from the environment variable `name`, or returns the value that
/// [`remove_password_env_vars`] moved out of it. An empty value counts as unset.
pub fn password_from_env(name: &str) -> anyhow::Result<Option<Zeroizing<String>>> {
    let removed = REMOVED_PASSWORD_ENV_VARS
        .get()
        .and_then(|vars| vars.iter().find(|(var, _)| *var == name));
    match removed {
        Some((_, value)) => value.clone().map_err(|e| anyhow!(e)),
        None => read_password_env_var(name),
    }
}

fn read_password_env_var(name: &str) -> anyhow::Result<Option<Zeroizing<String>>> {
    match std::env::var(name) {
        Ok(password) if password.is_empty() => Ok(None),
        Ok(password) => Ok(Some(Zeroizing::new(password))),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => bail!("{name} is not valid UTF-8"),
    }
}

/// Argon2id cost parameters.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Argon2Cost {
    pub(crate) memory_kib: u32,
    pub(crate) iterations: u32,
    pub(crate) parallelism: u32,
}

impl Argon2Cost {
    /// The second recommended option of RFC 9106, section 4: 64 MiB of memory, 3 passes and 4
    /// lanes. A key derivation with it takes a fraction of a second, which every command that
    /// unlocks the keystore pays once.
    pub(crate) const RECOMMENDED: Self = Self {
        memory_kib: 64 * 1024,
        iterations: 3,
        parallelism: 4,
    };
}

/// The JSON contents of an encrypted keystore file.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EncryptedKeystoreFile {
    version: u32,
    /// Base64 encoded `flag || pubkey` of every key in the keystore.
    public_keys: Vec<String>,
    kdf: KdfParams,
    cipher: CipherParams,
    /// Base64 encoding of the ciphertext followed by its 16-byte authentication tag.
    ciphertext: String,
}

/// The Argon2id parameters that derive the encryption key from the password.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct KdfParams {
    algorithm: String,
    version: u32,
    memory_kib: u32,
    iterations: u32,
    parallelism: u32,
    /// Base64 encoded salt.
    salt: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CipherParams {
    algorithm: String,
    /// Base64 encoded nonce.
    nonce: String,
}

/// The fields of an encrypted keystore file that the encryption authenticates in addition to the
/// ciphertext, in their BCS serialization.
#[derive(Serialize)]
struct AssociatedData<'a> {
    domain: &'a str,
    version: u32,
    kdf: &'a KdfParams,
    cipher_algorithm: &'a str,
    public_keys: &'a [String],
}

impl EncryptedKeystoreFile {
    /// Parses and validates the contents of an encrypted keystore file, and returns it with its
    /// public keys, which are validated where they are decoded.
    pub(crate) fn from_slice(contents: &[u8]) -> anyhow::Result<(Self, Vec<PublicKey>)> {
        // The version is checked before the other fields, so that a file in a newer format fails
        // with an error that names its version.
        #[derive(Deserialize)]
        struct FormatVersion {
            version: Option<u64>,
        }
        let FormatVersion { version } = serde_json::from_slice(contents)?;
        let version =
            version.ok_or_else(|| anyhow!("The encrypted keystore has no format version"))?;
        ensure!(
            version == u64::from(FORMAT_VERSION),
            "The keystore uses encryption format version {version}, which this version of Sui \
             does not support"
        );

        let file: Self = serde_json::from_slice(contents)?;
        file.kdf.argon2()?;
        ensure!(
            file.cipher.algorithm == CIPHER_ALGORITHM,
            "Unsupported keystore cipher {}",
            file.cipher.algorithm
        );
        decode_exact(&file.cipher.nonce, "nonce", NONCE_LENGTH)?;
        let ciphertext =
            Base64::decode(&file.ciphertext).map_err(|e| anyhow!("Invalid ciphertext: {e}"))?;
        ensure!(
            ciphertext.len() >= TAG_LENGTH,
            "Invalid ciphertext: it is shorter than the {TAG_LENGTH}-byte authentication tag"
        );
        let public_keys = file.public_keys()?;
        Ok((file, public_keys))
    }

    /// Returns the public keys, which the file stores in plaintext.
    pub(crate) fn public_keys(&self) -> anyhow::Result<Vec<PublicKey>> {
        let public_keys = self
            .public_keys
            .iter()
            .map(|key| {
                PublicKey::decode_base64(key).map_err(|e| anyhow!("Invalid public key {key}: {e}"))
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        // The check of the decrypted keys compares their number with the number of public keys,
        // which a repeated public key would make equal for a different set of keys.
        let addresses: BTreeSet<SuiAddress> = public_keys.iter().map(SuiAddress::from).collect();
        ensure!(
            addresses.len() == public_keys.len(),
            "The file lists a public key more than once"
        );
        Ok(public_keys)
    }

    fn associated_data(&self) -> anyhow::Result<Vec<u8>> {
        Ok(bcs::to_bytes(&AssociatedData {
            domain: ASSOCIATED_DATA_DOMAIN,
            version: self.version,
            kdf: &self.kdf,
            cipher_algorithm: &self.cipher.algorithm,
            public_keys: &self.public_keys,
        })?)
    }
}

impl KdfParams {
    fn generate(cost: Argon2Cost) -> Self {
        let mut salt = [0u8; SALT_LENGTH];
        OsRng.fill_bytes(&mut salt);
        Self {
            algorithm: KDF_ALGORITHM.to_string(),
            version: Version::V0x13.into(),
            memory_kib: cost.memory_kib,
            iterations: cost.iterations,
            parallelism: cost.parallelism,
            salt: Base64::encode(salt),
        }
    }

    /// Validates the parameters, and returns the Argon2id instance and the salt that they
    /// describe.
    fn argon2(&self) -> anyhow::Result<(Argon2<'static>, Vec<u8>)> {
        ensure!(
            self.algorithm == KDF_ALGORITHM,
            "Unsupported keystore key derivation function {}",
            self.algorithm
        );
        ensure!(
            self.version == u32::from(Version::V0x13),
            "Unsupported Argon2 version {}",
            self.version
        );
        ensure!(
            self.memory_kib <= MAX_MEMORY_KIB,
            "The Argon2 memory cost of {} KiB is larger than the maximum of {MAX_MEMORY_KIB} KiB",
            self.memory_kib
        );
        ensure!(
            self.iterations <= MAX_ITERATIONS,
            "The Argon2 iteration count of {} is larger than the maximum of {MAX_ITERATIONS}",
            self.iterations
        );
        ensure!(
            self.parallelism <= MAX_PARALLELISM,
            "The Argon2 parallelism of {} is larger than the maximum of {MAX_PARALLELISM}",
            self.parallelism
        );
        let params = Params::new(
            self.memory_kib,
            self.iterations,
            self.parallelism,
            Some(KEY_LENGTH),
        )
        .map_err(|e| anyhow!("Invalid Argon2 parameters: {e}"))?;
        let salt = Base64::decode(&self.salt).map_err(|e| anyhow!("Invalid salt: {e}"))?;
        ensure!(
            salt.len() >= SALT_LENGTH,
            "The salt is shorter than {SALT_LENGTH} bytes"
        );
        Ok((
            Argon2::new(Algorithm::Argon2id, Version::V0x13, params),
            salt,
        ))
    }
}

/// An AES-256 key derived from a keystore password, with the parameters that derived it.
#[derive(Clone)]
pub(crate) struct KeystoreKey {
    kdf: KdfParams,
    key: Zeroizing<[u8; KEY_LENGTH]>,
}

impl KeystoreKey {
    /// Derives a key from `password` with a new random salt.
    pub(crate) fn generate(password: &str, cost: Argon2Cost) -> anyhow::Result<Self> {
        ensure!(
            !password.is_empty(),
            "The keystore password cannot be empty"
        );
        Self::derive(password, KdfParams::generate(cost))
    }

    fn derive(password: &str, kdf: KdfParams) -> anyhow::Result<Self> {
        let (argon2, salt) = kdf.argon2()?;
        // Keyboards and input methods can compose the same characters differently, so the key is
        // derived from the Unicode NFC form of the password.
        let normalized;
        let password = if is_nfc(password) {
            password
        } else {
            normalized = Zeroizing::new(password.nfc().collect::<String>());
            normalized.as_str()
        };
        let block_count = argon2.params().block_count();
        let mut memory = Vec::new();
        memory
            .try_reserve_exact(block_count)
            .map_err(|_| anyhow!("Not enough memory to derive the keystore key"))?;
        memory.resize(block_count, Block::default());
        let mut memory = WipedBlocks(memory);
        let mut key = Zeroizing::new([0u8; KEY_LENGTH]);
        argon2
            .hash_password_into_with_memory(
                password.as_bytes(),
                &salt,
                key.as_mut_slice(),
                memory.0.as_mut_slice(),
            )
            .map_err(|e| anyhow!("Cannot derive the keystore key from the password: {e}"))?;
        Ok(Self { kdf, key })
    }

    /// Encrypts `keys` into the contents of an encrypted keystore file, with a new random nonce.
    pub(crate) fn encrypt(
        &self,
        keys: &BTreeMap<SuiAddress, SuiKeyPair>,
    ) -> anyhow::Result<EncryptedKeystoreFile> {
        let plaintext = encode_key_pairs(keys)?;
        let mut nonce = [0u8; NONCE_LENGTH];
        OsRng.fill_bytes(&mut nonce);
        let mut file = EncryptedKeystoreFile {
            version: FORMAT_VERSION,
            public_keys: keys
                .values()
                .map(|key| key.public().encode_base64())
                .collect(),
            kdf: self.kdf.clone(),
            cipher: CipherParams {
                algorithm: CIPHER_ALGORITHM.to_string(),
                nonce: Base64::encode(nonce),
            },
            ciphertext: String::new(),
        };
        let ciphertext = self.cipher()?.encrypt_authenticated(
            &InitializationVector::from_bytes(&nonce)?,
            &file.associated_data()?,
            plaintext.as_bytes(),
        );
        file.ciphertext = Base64::encode(ciphertext);
        Ok(file)
    }

    /// Decrypts the private keys of `file`. Returns `Ok(None)` if this key cannot decrypt the
    /// file: the file has other KDF parameters, the password that derived this key is not the
    /// password of the file, or the file was modified.
    pub(crate) fn decrypt(
        &self,
        file: &EncryptedKeystoreFile,
    ) -> anyhow::Result<Option<BTreeMap<SuiAddress, SuiKeyPair>>> {
        if self.kdf != file.kdf {
            return Ok(None);
        }
        let nonce = decode_exact(&file.cipher.nonce, "nonce", NONCE_LENGTH)?;
        let ciphertext =
            Base64::decode(&file.ciphertext).map_err(|e| anyhow!("Invalid ciphertext: {e}"))?;
        let Ok(plaintext) = self.cipher()?.decrypt_authenticated(
            &InitializationVector::from_bytes(&nonce)?,
            &file.associated_data()?,
            &ciphertext,
        ) else {
            return Ok(None);
        };
        let plaintext = Zeroizing::new(plaintext);

        let encoded: Vec<Zeroizing<String>> = serde_json::from_slice(&plaintext)?;
        let keys = decode_key_pairs(&encoded)?;
        let public_keys = file.public_keys()?;
        ensure!(
            public_keys.len() == keys.len()
                && public_keys.iter().all(|public_key| {
                    keys.get(&SuiAddress::from(public_key))
                        .is_some_and(|key| key.public() == *public_key)
                }),
            "The private keys in the keystore file do not match its public keys"
        );
        Ok(Some(keys))
    }

    fn cipher(&self) -> anyhow::Result<Aes256Gcm<U12>> {
        Ok(Aes256Gcm::new(AesKey::from_bytes(self.key.as_slice())?))
    }
}

/// Argon2 memory blocks, which are wiped once when they are dropped.
struct WipedBlocks(Vec<Block>);

impl Drop for WipedBlocks {
    fn drop(&mut self) {
        for block in &mut self.0 {
            block.zeroize();
        }
    }
}

/// The private keys of an unlocked keystore, with the key that encrypts them.
pub(crate) struct UnlockedKeys {
    pub(crate) keys: BTreeMap<SuiAddress, SuiKeyPair>,
    pub(crate) key: KeystoreKey,
}

impl Clone for UnlockedKeys {
    fn clone(&self) -> Self {
        Self {
            keys: self
                .keys
                .iter()
                .map(|(address, key)| (*address, key.copy()))
                .collect(),
            key: self.key.clone(),
        }
    }
}

/// The private keys of an encrypted keystore, or the error of the unlock that failed.
#[derive(Default)]
pub(crate) struct UnlockState {
    pub(crate) keys: OnceCell<UnlockedKeys>,
    /// The error of the first unlock with the environment variable or the prompt that failed.
    /// Later unlocks fail with it, so that they do not ask for the password or derive the key
    /// again. [`crate::keystore::FileBasedKeystore::unlock`] still accepts the password.
    pub(crate) error: OnceCell<String>,
}

impl UnlockState {
    pub(crate) fn unlocked(keys: UnlockedKeys) -> Self {
        Self {
            keys: OnceCell::with_value(keys),
            error: OnceCell::new(),
        }
    }

    /// Returns the private keys, or unlocks the keystore with `unlock` if no unlock ran before.
    /// Concurrent callers wait for a single unlock.
    pub(crate) fn get_or_unlock(
        &self,
        unlock: impl FnOnce() -> Result<UnlockedKeys, anyhow::Error>,
    ) -> Result<&UnlockedKeys, anyhow::Error> {
        self.keys.get_or_try_init(|| {
            if let Some(error) = self.error.get() {
                bail!("{error}");
            }
            unlock().inspect_err(|error| {
                let _ = self.error.set(format!("{error:#}"));
            })
        })
    }
}

/// Decrypts `file` with `password`. Returns `Ok(None)` if the password is incorrect.
pub(crate) fn decrypt_with_password(
    file: &EncryptedKeystoreFile,
    password: &str,
) -> anyhow::Result<Option<UnlockedKeys>> {
    let key = KeystoreKey::derive(password, file.kdf.clone())?;
    Ok(key.decrypt(file)?.map(|keys| UnlockedKeys { keys, key }))
}

/// Returns the error for a password that does not decrypt the keystore at `path`.
pub(crate) fn incorrect_password(path: &Path) -> anyhow::Error {
    anyhow!(
        "Cannot decrypt the keystore at {}: the password is incorrect, or the file is damaged",
        path.display()
    )
}

/// Decrypts `file`, the contents of the keystore file at `path`, with the password from
/// [`KEYSTORE_PASSWORD_ENV_VAR`] or, if that is not set, with the password prompt.
pub(crate) fn unlock(file: &EncryptedKeystoreFile, path: &Path) -> anyhow::Result<UnlockedKeys> {
    let prompt = PASSWORD_PROMPT.get();
    unlock_with(
        file,
        path,
        password_from_env(KEYSTORE_PASSWORD_ENV_VAR)?,
        prompt.map(|prompt| prompt as &PromptFn<'_>),
    )
}

fn unlock_with(
    file: &EncryptedKeystoreFile,
    path: &Path,
    env_password: Option<Zeroizing<String>>,
    prompt: Option<&PromptFn<'_>>,
) -> anyhow::Result<UnlockedKeys> {
    if let Some(password) = env_password {
        return decrypt_with_password(file, &password)?.ok_or_else(|| {
            anyhow!(
                "Cannot decrypt the keystore at {} with the password in \
                 {KEYSTORE_PASSWORD_ENV_VAR}: the password is incorrect, or the file is damaged",
                path.display()
            )
        });
    }

    let Some(prompt) = prompt else {
        bail!(
            "The keystore at {} is encrypted. Set the {KEYSTORE_PASSWORD_ENV_VAR} environment \
             variable to unlock it",
            path.display()
        );
    };
    let locked = || {
        anyhow!(
            "The keystore at {} is encrypted. Set the {KEYSTORE_PASSWORD_ENV_VAR} environment \
             variable, or run the command in an interactive terminal, to unlock it",
            path.display()
        )
    };
    let message = format!("Password for the keystore at {}:", path.display());
    for attempt in 1..=MAX_PROMPT_ATTEMPTS {
        let password = Zeroizing::new(prompt(&message)?.ok_or_else(locked)?);
        if let Some(unlocked) = decrypt_with_password(file, &password)? {
            return Ok(unlocked);
        }
        if attempt < MAX_PROMPT_ATTEMPTS {
            eprintln!("Incorrect password.");
        }
    }
    Err(incorrect_password(path))
}

/// Returns whether `contents` has the format of an encrypted keystore file, a JSON object, rather
/// than the JSON array of a plaintext keystore file.
pub fn is_encrypted_file(contents: &[u8]) -> bool {
    contents.iter().find(|byte| !byte.is_ascii_whitespace()) == Some(&b'{')
}

/// Decodes the Base64 string `value` and checks that it is `length` bytes long.
fn decode_exact(value: &str, name: &str, length: usize) -> anyhow::Result<Vec<u8>> {
    let bytes = Base64::decode(value).map_err(|e| anyhow!("Invalid {name}: {e}"))?;
    ensure!(
        bytes.len() == length,
        "The {name} must be {length} bytes long"
    );
    Ok(bytes)
}
