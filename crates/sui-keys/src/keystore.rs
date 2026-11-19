// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::encryption::{
    self, Argon2Cost, EncryptedKeystoreFile, KeystoreKey, UnlockState, UnlockedKeys,
};
pub use crate::external::External;
use crate::external::ProvisionMode;
use crate::key_derive::{derive_key_pair_from_path, generate_new_key};
use crate::key_identity::KeyIdentity;
use crate::random_names::{random_name, random_names};
use mysten_common::ZipDebugEqIteratorExt;

use anyhow::{Context, anyhow, bail, ensure};
use async_trait::async_trait;
use bip32::DerivationPath;
use bip39::{Language, Mnemonic, Seed};
#[cfg(unix)]
use colored::Colorize as _;
use fastcrypto::hash::HashFunction;
use rand::{SeedableRng, rngs::StdRng};
use regex::Regex;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use shared_crypto::intent::{Intent, IntentMessage};
use std::collections::{BTreeMap, HashSet};
use std::fmt::Write;
use std::fmt::{Display, Formatter};
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use sui_types::base_types::SuiAddress;
use sui_types::crypto::{DefaultHash, get_key_pair_from_rng};
use sui_types::crypto::{
    EncodeDecodeBase64, PublicKey, Signature, SignatureScheme, SuiKeyPair, enum_dispatch,
};
use zeroize::Zeroizing;

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

#[derive(Default)]
pub enum GenerateOptions {
    /// Default options for key generation of any keystore type.
    #[default]
    Default,
    /// File or InMem keystore
    Local(LocalGenerate),
    /// Signer
    ExternalSigner {
        signer: String,
        provision_mode: Option<ProvisionMode>,
    },
}

#[derive(Debug)]
pub struct GeneratedKey {
    pub address: SuiAddress,
    pub public_key: PublicKey,
    pub scheme: SignatureScheme,
    pub mnemonic: Option<String>,
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
            GenerateOptions::ExternalSigner { .. } => {
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
            mnemonic: None,
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

    /// Makes the private keys available for signing. An encrypted [`FileBasedKeystore`] asks for
    /// its password now, so that later signatures do not need it. Other keystores do nothing.
    ///
    /// The password prompt and the key derivation run on a blocking thread, so they do not block
    /// the async runtime. The methods that are not async, such as [`Self::export`], unlock a
    /// locked keystore on the calling thread. A service calls this before it starts to serve
    /// requests, because it cannot ask for the password later.
    async fn ensure_unlocked(&self) -> Result<(), anyhow::Error> {
        Ok(())
    }

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
    fn get_by_identity(&self, key_identity: &KeyIdentity) -> Result<SuiAddress, anyhow::Error> {
        match key_identity {
            KeyIdentity::Address(addr) => Ok(*addr),
            KeyIdentity::Alias(alias) => Ok(*self
                .addresses_with_alias()
                .iter()
                .find(|(_, a)| a.alias == *alias)
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
    keys: StoredKeys,
    aliases: BTreeMap<SuiAddress, Alias>,
    path: Option<PathBuf>,
    known_file: KnownFile,
    known_aliases_file: KnownFile,
}

/// The keystore file or the aliases file as a [`FileBasedKeystore`] last read or wrote it. A write
/// fails if another process changed the file since then, so that the write does not undo their
/// change. No lock protects the file, so a change that another process makes between the check
/// and the write is lost, as it is without the check.
#[derive(Default)]
struct KnownFile(Mutex<FileDigest>);

/// The state of a file that a write expects to find.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum FileDigest {
    /// The keystore did not read the file, so a write replaces the file whatever it holds.
    #[default]
    Unknown,
    Missing,
    Contents([u8; 32]),
}

impl FileDigest {
    fn of(contents: &[u8]) -> Self {
        Self::Contents(DefaultHash::digest(contents).digest)
    }

    fn of_file(path: &Path) -> std::io::Result<Self> {
        match fs::read(path) {
            Ok(contents) => Ok(Self::of(&Zeroizing::new(contents))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::Missing),
            Err(e) => Err(e),
        }
    }
}

impl KnownFile {
    fn new(digest: FileDigest) -> Self {
        Self(Mutex::new(digest))
    }

    fn digest(&self) -> FileDigest {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn forget(&mut self) {
        *self.0.get_mut().unwrap_or_else(PoisonError::into_inner) = FileDigest::Unknown;
    }

    /// Fails if another process changed the file at `path` since the keystore read or wrote it.
    /// Callers ask for passwords and derive keys before they write, so that this check also
    /// covers the time that those take.
    fn check_unchanged(&self, path: &Path) -> Result<(), anyhow::Error> {
        let expected = self.digest();
        ensure!(
            expected == FileDigest::Unknown || FileDigest::of_file(path)? == expected,
            "The file {} changed after this command read it. Run the command again",
            path.display()
        );
        Ok(())
    }

    /// Writes `contents` to the keystore file at `path`. If `check` is set, the write fails when
    /// the check of [`Self::check_unchanged`] fails. If `sync` is set, the contents are on the disk
    /// when the write returns.
    async fn write_with(
        &self,
        path: &Path,
        contents: impl AsRef<[u8]> + Send + 'static,
        check: bool,
        sync: bool,
    ) -> Result<(), anyhow::Error> {
        if check {
            self.check_unchanged(path)?;
        }
        let digest = FileDigest::of(contents.as_ref());
        let result = write_keystore_file(path, contents, sync).await;
        self.record_write(path, digest, result.is_ok());
        result
    }

    /// Writes `contents` to the keystore file at `path` as [`Self::write_with`] does, after it
    /// writes a recovery copy of the keystore next to the keystore file and flushes both to the
    /// disk. If the write fails after it truncates the keystore file, or a crash interrupts it, the
    /// recovery copy holds the keystore that `copy` selects. The copy is removed once the keystore
    /// file holds the new contents, or if the failed write left the keystore file unchanged.
    async fn write_with_recovery_copy(
        &self,
        path: &Path,
        contents: impl AsRef<[u8]> + Send + 'static,
        copy: RecoveryCopy,
    ) -> Result<(), anyhow::Error> {
        self.check_unchanged(path)?;
        let copy_path = recovery_copy_path(path);
        let copy_contents = match copy {
            RecoveryCopy::NewContents => contents.as_ref().to_vec(),
            RecoveryCopy::OldContents => fs::read(path).with_context(|| {
                format!(
                    "Cannot read the keystore file to copy it: {}",
                    path.display()
                )
            })?,
        };
        if let Err(error) = write_file_durably(&copy_path, copy_contents).await {
            let _ = fs::remove_file(&copy_path);
            // A keystore file can be writable in a directory that is not. Any other error, such
            // as a full disk, would also make the write of the keystore file fail after it
            // truncates the file, so the keystore file is not written then.
            if !matches!(
                error.kind(),
                std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::ReadOnlyFilesystem
            ) {
                return Err(anyhow::Error::new(error).context(format!(
                    "Cannot write the recovery copy of the keystore to {}",
                    copy_path.display()
                )));
            }
            eprintln!(
                "Cannot write a recovery copy of the keystore to {}: {error}. The keystore file \
                 is rewritten without one.",
                copy_path.display()
            );
            return self.write_with(path, contents, true, true).await;
        }
        // Writing and flushing the copy takes time, so the check runs again right before the
        // write. A refused write needs no copy.
        if let Err(error) = self.check_unchanged(path) {
            remove_recovery_copy(&copy_path);
            return Err(error);
        }
        let before = FileDigest::of_file(path).ok();
        let result = self.write_with(path, contents, false, true).await;
        if result.is_ok() || (before.is_some() && FileDigest::of_file(path).ok() == before) {
            remove_recovery_copy(&copy_path);
            return result;
        }
        let held = match copy {
            RecoveryCopy::NewContents => "the new keystore",
            RecoveryCopy::OldContents => "the keystore as it was before this command",
        };
        result.with_context(|| {
            format!(
                "The keystore file {} can be damaged. The recovery copy {} holds {held}: replace \
                 the keystore file with it",
                path.display(),
                copy_path.display()
            )
        })
    }

    /// Records what the file at `path` holds after a write of the contents with `digest`.
    fn record_write(&self, path: &Path, digest: FileDigest, succeeded: bool) {
        let digest = if succeeded {
            Some(digest)
        } else {
            // A write that cannot open the file leaves it unchanged, and a write that fails later
            // can leave it truncated. The digest of what the file holds now lets the next write
            // replace a truncated file, and still detects a later change by another process.
            FileDigest::of_file(path).ok()
        };
        if let Some(digest) = digest {
            *self.0.lock().unwrap_or_else(PoisonError::into_inner) = digest;
        }
    }
}

/// The keystore that the recovery copy of a conversion holds. It is always encrypted, so that a
/// copy that a crash leaves behind does not hold the private keys in plaintext.
#[derive(Clone, Copy)]
enum RecoveryCopy {
    /// The new contents, for a conversion that writes an encrypted keystore.
    NewContents,
    /// The contents before the conversion, for a conversion that writes a plaintext keystore.
    OldContents,
}

/// The keys of a [`FileBasedKeystore`], in the format of its file.
enum StoredKeys {
    Plaintext(BTreeMap<SuiAddress, SuiKeyPair>),
    Encrypted(Box<EncryptedKeys>),
}

/// The keys of a keystore whose file is encrypted with a password.
///
/// Until the keystore is unlocked, its keys are `public_keys`. After that, they are the keys in
/// `unlock`, which change as keys are imported and removed.
struct EncryptedKeys {
    /// The path of the keystore file, for the messages that ask for its password.
    path: PathBuf,
    /// The public keys that the file stored when it was read.
    public_keys: BTreeMap<SuiAddress, PublicKey>,
    /// The file contents as they were read or last re-encrypted with a new password. Unlocking
    /// decrypts them, and a locked keystore saves them to a new path.
    file: EncryptedKeystoreFile,
    /// The result of unlocking the keystore. The blocking task that unlocks it for an async
    /// caller holds a reference to it.
    unlock: Arc<UnlockState>,
    /// Lets one async caller at a time start an unlock, while the others wait for its result
    /// without blocking their threads.
    unlocking: tokio::sync::Mutex<()>,
}

impl Default for StoredKeys {
    fn default() -> Self {
        Self::Plaintext(BTreeMap::new())
    }
}

impl StoredKeys {
    fn public_keys(&self) -> Vec<PublicKey> {
        match self {
            Self::Plaintext(keys) => keys.values().map(|key| key.public()).collect(),
            Self::Encrypted(encrypted) => match encrypted.unlock.keys.get() {
                Some(unlocked) => unlocked.keys.values().map(|key| key.public()).collect(),
                None => encrypted.public_keys.values().cloned().collect(),
            },
        }
    }

    fn contains(&self, address: &SuiAddress) -> bool {
        match self {
            Self::Plaintext(keys) => keys.contains_key(address),
            Self::Encrypted(encrypted) => match encrypted.unlock.keys.get() {
                Some(unlocked) => unlocked.keys.contains_key(address),
                None => encrypted.public_keys.contains_key(address),
            },
        }
    }

    /// Returns the private keys, unlocking an encrypted keystore with the password from
    /// [`encryption::KEYSTORE_PASSWORD_ENV_VAR`] or from the password prompt if it is locked.
    fn private_keys(&self) -> Result<&BTreeMap<SuiAddress, SuiKeyPair>, anyhow::Error> {
        match self {
            Self::Plaintext(keys) => Ok(keys),
            Self::Encrypted(encrypted) => Ok(&encrypted.unlocked()?.keys),
        }
    }

    fn private_keys_mut(&mut self) -> Result<&mut BTreeMap<SuiAddress, SuiKeyPair>, anyhow::Error> {
        match self {
            Self::Plaintext(keys) => Ok(keys),
            Self::Encrypted(encrypted) => Ok(&mut encrypted.unlocked_mut()?.keys),
        }
    }

    /// Unlocks an encrypted keystore if it is locked, as [`Self::private_keys`] does, but with the
    /// password prompt and the key derivation on a blocking thread.
    async fn unlock_in_background(&self) -> Result<(), anyhow::Error> {
        match self {
            Self::Plaintext(_) => Ok(()),
            Self::Encrypted(encrypted) => encrypted.unlock_in_background().await,
        }
    }
}

impl EncryptedKeys {
    /// Returns the keys of the encrypted keystore `file` at `path`, whose decoded public keys are
    /// `public_keys`.
    fn new(path: &Path, file: EncryptedKeystoreFile, public_keys: Vec<PublicKey>) -> Self {
        Self {
            path: path.to_path_buf(),
            public_keys: public_keys
                .into_iter()
                .map(|key| (SuiAddress::from(&key), key))
                .collect(),
            file,
            unlock: Arc::default(),
            unlocking: tokio::sync::Mutex::default(),
        }
    }

    fn new_unlocked(path: &Path, file: EncryptedKeystoreFile, unlocked: UnlockedKeys) -> Self {
        Self {
            path: path.to_path_buf(),
            public_keys: unlocked
                .keys
                .iter()
                .map(|(address, key)| (*address, key.public()))
                .collect(),
            file,
            unlock: Arc::new(UnlockState::unlocked(unlocked)),
            unlocking: tokio::sync::Mutex::default(),
        }
    }

    /// Returns the private keys, unlocking the keystore on this thread if it is locked.
    fn unlocked(&self) -> Result<&UnlockedKeys, anyhow::Error> {
        self.unlock
            .get_or_unlock(|| encryption::unlock(&self.file, &self.path))
    }

    fn unlocked_mut(&mut self) -> Result<&mut UnlockedKeys, anyhow::Error> {
        self.unlocked()?;
        // The blocking task of an async unlock whose caller stopped waiting for it can still hold
        // the state. The keys are then copied into a state that this keystore holds alone.
        if Arc::get_mut(&mut self.unlock).is_none() {
            let keys = self.unlocked()?.clone();
            self.unlock = Arc::new(UnlockState::unlocked(keys));
        }
        Arc::get_mut(&mut self.unlock)
            .and_then(|state| state.keys.get_mut())
            .ok_or_else(|| anyhow!("The keystore at {} is locked", self.path.display()))
    }

    /// Unlocks the keystore as [`Self::unlocked`] does, but on a blocking thread, so that an async
    /// caller does not block its runtime while the prompt waits for the password or the key is
    /// derived.
    async fn unlock_in_background(&self) -> Result<(), anyhow::Error> {
        if self.unlock.keys.get().is_none() {
            let _unlocking = self.unlocking.lock().await;
            if self.unlock.keys.get().is_none() && self.unlock.error.get().is_none() {
                let (state, file, path) =
                    (self.unlock.clone(), self.file.clone(), self.path.clone());
                tokio::task::spawn_blocking(move || {
                    state
                        .get_or_unlock(|| encryption::unlock(&file, &path))
                        .map(|_| ())
                })
                .await??;
            }
        }
        self.unlocked().map(|_| ())
    }

    /// Unlocks the keystore with `password`, or checks the password if it is already unlocked.
    fn unlock_with_password(&self, password: &str) -> Result<(), anyhow::Error> {
        let unlocked = encryption::decrypt_with_password(&self.file, password)?
            .ok_or_else(|| encryption::incorrect_password(&self.path))?;
        // An unlocked keystore keeps its keys, which can include changes since it was read.
        let _ = self.unlock.keys.set(unlocked);
        Ok(())
    }
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
        // The alternate format includes the causes of the error, such as the reason that an
        // encrypted keystore file is invalid.
        FileBasedKeystore::load_or_create(&PathBuf::from(String::deserialize(deserializer)?))
            .map_err(|e| D::Error::custom(format!("{e:#}")))
    }
}

#[async_trait]
impl AccountKeystore for FileBasedKeystore {
    async fn sign_hashed(
        &self,
        address: &SuiAddress,
        msg: &[u8],
    ) -> Result<Signature, signature::Error> {
        let key = self
            .private_key_in_background(address)
            .await
            .map_err(signature::Error::from_source)?;
        Ok(Signature::new_hashed(msg, key))
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
        let key = self
            .private_key_in_background(address)
            .await
            .map_err(signature::Error::from_source)?;
        Ok(Signature::new_secure(&IntentMessage::new(intent, msg), key))
    }

    async fn import(
        &mut self,
        alias: Option<String>,
        keypair: SuiKeyPair,
    ) -> Result<(), anyhow::Error> {
        let address: SuiAddress = (&keypair.public()).into();
        let alias = self.create_alias(alias)?;
        let public_key_base64 = keypair.public().encode_base64();
        self.keys.unlock_in_background().await?;
        let previous_key = self.keys.private_keys_mut()?.insert(address, keypair);
        let previous_alias = self.aliases.insert(
            address,
            Alias {
                alias,
                public_key_base64,
            },
        );
        self.save_or_restore(move |keystore| {
            // The keystore is unlocked, so this does not ask for the password.
            if let Ok(keys) = keystore.keys.private_keys_mut() {
                match previous_key {
                    Some(key) => keys.insert(address, key),
                    None => keys.remove(&address),
                };
            }
            match previous_alias {
                Some(alias) => keystore.aliases.insert(address, alias),
                None => keystore.aliases.remove(&address),
            };
        })
        .await
    }

    async fn remove(&mut self, address: SuiAddress) -> Result<(), anyhow::Error> {
        let removed_key = if self.keys.contains(&address) {
            self.keys.unlock_in_background().await?;
            self.keys.private_keys_mut()?.remove(&address)
        } else {
            None
        };
        let removed_alias = self.aliases.remove(&address);
        self.save_or_restore(move |keystore| {
            if let Some(key) = removed_key
                && let Ok(keys) = keystore.keys.private_keys_mut()
            {
                keys.insert(address, key);
            }
            if let Some(alias) = removed_alias {
                keystore.aliases.insert(address, alias);
            }
        })
        .await
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
        self.keys.public_keys()
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
        self.private_key(address)
    }

    /// Decrypts the private keys of an encrypted keystore with the password from the
    /// `SUI_KEYSTORE_PASSWORD` environment variable or, if it is not set, from the password
    /// prompt. Operations that need the private keys of a locked keystore do this first. Does
    /// nothing if the keystore is not encrypted or is already unlocked.
    async fn ensure_unlocked(&self) -> Result<(), anyhow::Error> {
        self.keys.unlock_in_background().await
    }

    /// Updates an old alias to the new alias and saves it to the alias file.
    /// If the new_alias is None, it will generate a new random alias.
    async fn update_alias(
        &mut self,
        old_alias: &str,
        new_alias: Option<&str>,
    ) -> Result<String, anyhow::Error> {
        let aliases = self.aliases.clone();
        let new_alias_name = self.update_alias_value(old_alias, new_alias)?;
        if let Err(error) = self.save_aliases().await {
            // The aliases stay as the aliases file holds them, so that a retry starts from there.
            self.aliases = aliases;
            return Err(error);
        }
        Ok(new_alias_name)
    }
}

impl FileBasedKeystore {
    pub fn load_or_create(path: &PathBuf) -> Result<Self, anyhow::Error> {
        let (keys, file_digest) = if path.exists() {
            #[cfg(unix)]
            let _ = set_reduced_file_permissions(path).inspect_err(|error| {
                eprintln!(
                    "[{}]: while attempting to ensure reduced file permissions on '{}'. Cannot set \
                    permissions for keystore file. Error: {error}",
                    "warning".bold().yellow(),
                    path.display(),
                );
            });

            let contents =
                Zeroizing::new(fs::read(path).with_context(|| {
                    format!("Cannot open the keystore file: {}", path.display())
                })?);
            let keys = parse_keystore_file(path, &contents).map_err(|error| {
                let copy_path = recovery_copy_path(path);
                if copy_path.exists() {
                    error.context(format!(
                        "A recovery copy of the keystore from an interrupted `sui keytool` \
                         command is at {}. If the keystore file is damaged, replace it with \
                         that copy",
                        copy_path.display()
                    ))
                } else {
                    error
                }
            })?;
            (keys, FileDigest::of(&contents))
        } else {
            (StoredKeys::default(), FileDigest::Missing)
        };
        let public_keys = keys.public_keys();

        // check aliases
        let aliases_path = aliases_path(path);

        let (aliases, aliases_digest) = if aliases_path.exists() {
            let contents = fs::read(&aliases_path).with_context(|| {
                format!(
                    "Cannot open aliases file in keystore: {}",
                    aliases_path.display()
                )
            })?;

            let aliases: Vec<Alias> = serde_json::from_slice(&contents).with_context(|| {
                format!(
                    "Cannot deserialize aliases file in keystore: {}",
                    aliases_path.display(),
                )
            })?;

            let aliases = aliases
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
                })?;
            (aliases, FileDigest::of(&contents))
        } else if public_keys.is_empty() {
            (BTreeMap::new(), FileDigest::Missing)
        } else {
            let names: Vec<String> = random_names(HashSet::new(), public_keys.len());
            let aliases = public_keys
                .iter()
                .zip_debug_eq(names)
                .map(|(public_key, alias)| {
                    (
                        SuiAddress::from(public_key),
                        Alias {
                            alias,
                            public_key_base64: public_key.encode_base64(),
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

            fs::write(aliases_path, &aliases_store)?;
            (aliases, FileDigest::of(aliases_store.as_bytes()))
        };

        Ok(Self {
            keys,
            aliases,
            path: Some(path.to_path_buf()),
            known_file: KnownFile::new(file_digest),
            known_aliases_file: KnownFile::new(aliases_digest),
        })
    }

    /// Sets the file that later saves write the keystore to. An encrypted keystore keeps
    /// decrypting the contents of the file that it was read from, and a save writes them to the new
    /// path.
    pub fn set_path(&mut self, path: &Path) {
        if !self
            .path
            .as_deref()
            .is_some_and(|old_path| is_same_file(old_path, path))
        {
            self.known_file.forget();
            self.known_aliases_file.forget();
        }
        self.path = Some(path.to_path_buf());
    }

    /// Returns `None` for a keystore that has no file, such as [`FileBasedKeystore::default`].
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Returns whether the keystore file stores the private keys encrypted with a password.
    pub fn is_encrypted(&self) -> bool {
        matches!(self.keys, StoredKeys::Encrypted(_))
    }

    /// Decrypts the private keys of an encrypted keystore with `password`, so that later
    /// operations do not need the password. Fails if `password` is not the password of the
    /// keystore file as this keystore read it, also when the keystore is already unlocked. Does
    /// nothing if the keystore is not encrypted.
    pub fn unlock(&self, password: &str) -> Result<(), anyhow::Error> {
        match &self.keys {
            StoredKeys::Plaintext(_) => Ok(()),
            StoredKeys::Encrypted(encrypted) => encrypted.unlock_with_password(password),
        }
    }

    /// Encrypts the private keys with `password` and rewrites the keystore file. Later operations
    /// that need the private keys ask for the password, or read it from the
    /// `SUI_KEYSTORE_PASSWORD` environment variable.
    pub async fn encrypt(&mut self, password: &str) -> Result<(), anyhow::Error> {
        self.encrypt_with_cost(password, Argon2Cost::RECOMMENDED)
            .await
    }

    pub(crate) async fn encrypt_with_cost(
        &mut self,
        password: &str,
        cost: Argon2Cost,
    ) -> Result<(), anyhow::Error> {
        let Some(path) = &self.path else {
            bail!("Cannot encrypt a keystore that has no file");
        };
        let StoredKeys::Plaintext(keys) = &mut self.keys else {
            bail!("The keystore is already encrypted");
        };
        let key = generate_key_in_background(password, cost).await?;
        let file = key.encrypt(keys)?;
        self.known_file
            .write_with_recovery_copy(
                path,
                serde_json::to_vec_pretty(&file)?,
                RecoveryCopy::NewContents,
            )
            .await?;
        let keys = std::mem::take(keys);
        self.keys = StoredKeys::Encrypted(Box::new(EncryptedKeys::new_unlocked(
            path,
            file,
            UnlockedKeys { keys, key },
        )));
        Ok(())
    }

    /// Rewrites an encrypted keystore file with the private keys in plaintext. A locked keystore
    /// is unlocked first, as in [`AccountKeystore::ensure_unlocked`].
    pub async fn decrypt(&mut self) -> Result<(), anyhow::Error> {
        let StoredKeys::Encrypted(encrypted) = &mut self.keys else {
            bail!("The keystore is not encrypted");
        };
        let Some(path) = &self.path else {
            bail!("Cannot decrypt a keystore that has no file");
        };
        encrypted.unlock_in_background().await?;
        let keys = &mut encrypted.unlocked_mut()?.keys;
        self.known_file
            .write_with_recovery_copy(path, encode_key_pairs(keys)?, RecoveryCopy::OldContents)
            .await?;
        let keys = std::mem::take(keys);
        self.keys = StoredKeys::Plaintext(keys);
        Ok(())
    }

    /// Re-encrypts an encrypted keystore file with `new_password` and a new salt. A locked
    /// keystore is unlocked with the current password first, as in
    /// [`AccountKeystore::ensure_unlocked`].
    pub async fn change_password(&mut self, new_password: &str) -> Result<(), anyhow::Error> {
        self.change_password_with_cost(new_password, Argon2Cost::RECOMMENDED)
            .await
    }

    pub(crate) async fn change_password_with_cost(
        &mut self,
        new_password: &str,
        cost: Argon2Cost,
    ) -> Result<(), anyhow::Error> {
        let StoredKeys::Encrypted(encrypted) = &mut self.keys else {
            bail!("The keystore is not encrypted");
        };
        let Some(path) = &self.path else {
            bail!("Cannot change the password of a keystore that has no file");
        };
        encrypted.unlock_in_background().await?;
        let key = generate_key_in_background(new_password, cost).await?;
        let unlocked = encrypted.unlocked_mut()?;
        let file = key.encrypt(&unlocked.keys)?;
        self.known_file
            .write_with_recovery_copy(
                path,
                serde_json::to_vec_pretty(&file)?,
                RecoveryCopy::NewContents,
            )
            .await?;
        unlocked.key = key;
        encrypted.file = file;
        Ok(())
    }

    /// Returns the private key of `address` as [`Self::private_key`] does, but unlocks a locked
    /// keystore on a blocking thread.
    async fn private_key_in_background(
        &self,
        address: &SuiAddress,
    ) -> Result<&SuiKeyPair, anyhow::Error> {
        // The address is checked first, so that an unknown address does not ask for the password.
        ensure!(
            self.keys.contains(address),
            "Cannot find key for address: [{address}]"
        );
        self.keys.unlock_in_background().await?;
        self.private_key(address)
    }

    /// Returns the private key of `address`, unlocking the keystore first if it is locked.
    fn private_key(&self, address: &SuiAddress) -> Result<&SuiKeyPair, anyhow::Error> {
        // The address is checked first, so that an unknown address does not ask for the password.
        ensure!(
            self.keys.contains(address),
            "Cannot find key for address: [{address}]"
        );
        self.keys
            .private_keys()?
            .get(address)
            .ok_or_else(|| anyhow!("Cannot find key for address: [{address}]"))
    }

    pub async fn save_aliases(&self) -> Result<(), anyhow::Error> {
        self.check_files_unchanged()?;
        self.write_aliases().await
    }

    /// Fails if another process changed the keystore file or the aliases file since this keystore
    /// read or wrote them. If another process added keys to the keystore file, it also added
    /// aliases for them, which a write of the aliases would remove.
    fn check_files_unchanged(&self) -> Result<(), anyhow::Error> {
        if let Some(path) = &self.path {
            self.known_file.check_unchanged(path)?;
            self.known_aliases_file
                .check_unchanged(&aliases_path(path))?;
        }
        Ok(())
    }

    /// Writes the aliases file, which callers check with [`Self::check_files_unchanged`] first.
    async fn write_aliases(&self) -> Result<(), anyhow::Error> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let aliases_store = serde_json::to_string_pretty(
            &self.aliases.values().collect::<Vec<_>>(),
        )
        .with_context(|| {
            format!(
                "Cannot serialize aliases to file in keystore: {}",
                path.display()
            )
        })?;
        let aliases_path = aliases_path(path);
        let digest = FileDigest::of(aliases_store.as_bytes());
        let write_path = aliases_path.clone();
        // no reactor for tokio::fs::write in simtest, so we use spawn_blocking
        let result = tokio::task::spawn_blocking(move || fs::write(write_path, aliases_store))
            .await?
            .with_context(|| format!("Cannot write aliases to file: {}", path.display()));
        self.known_aliases_file
            .record_write(&aliases_path, digest, result.is_ok());
        result
    }

    /// Keys saved as Base64 with 33 bytes `flag || privkey` ($BASE64_STR). An encrypted keystore
    /// saves them encrypted with its password.
    /// To see Bech32 format encoding, use `sui keytool export $SUI_ADDRESS` where
    /// $SUI_ADDRESS can be found with `sui keytool list`. Or use `sui keytool convert $BASE64_STR`
    pub async fn save_keystore(&self) -> Result<(), anyhow::Error> {
        self.write_keystore(true).await
    }

    /// Writes the keystore file, after the check of [`KnownFile::check_unchanged`] if `check` is
    /// set. Callers that checked the file just before skip the check.
    async fn write_keystore(&self, check: bool) -> Result<(), anyhow::Error> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        match &self.keys {
            StoredKeys::Plaintext(keys) => {
                let store = encode_key_pairs(keys).with_context(|| {
                    format!("Cannot serialize keystore to file: {}", path.display())
                })?;
                self.known_file.write_with(path, store, check, false).await
            }
            StoredKeys::Encrypted(encrypted) => match encrypted.unlock.keys.get() {
                Some(unlocked) => {
                    let file = unlocked.key.encrypt(&unlocked.keys)?;
                    self.known_file
                        .write_with(path, serde_json::to_vec_pretty(&file)?, check, false)
                        .await
                }
                // The keys of a locked keystore cannot have changed, so its file needs no write,
                // unless the keystore moved to another file.
                None if self.known_file.digest() != FileDigest::Unknown => Ok(()),
                None => {
                    self.known_file
                        .write_with(
                            path,
                            serde_json::to_vec_pretty(&encrypted.file)?,
                            check,
                            false,
                        )
                        .await
                }
            },
        }
    }

    /// Saves the keystore file and then the aliases file. Both files are checked before either is
    /// written, so a save that another process's change refuses writes neither of them.
    pub async fn save(&self) -> Result<(), anyhow::Error> {
        self.check_files_unchanged()?;
        self.write_keystore(false).await?;
        self.write_aliases().await
    }

    /// Saves the keystore as [`Self::save`] does. If the keystore file is not saved, `restore`
    /// undoes the change in memory, so that the keystore still matches its files and a retry
    /// starts from them. A failed write of the aliases file after it keeps the change, which the
    /// keystore file then holds.
    async fn save_or_restore(
        &mut self,
        restore: impl FnOnce(&mut Self) + Send,
    ) -> Result<(), anyhow::Error> {
        let saved = match self.check_files_unchanged() {
            Ok(()) => self.write_keystore(false).await,
            Err(error) => Err(error),
        };
        if let Err(error) = saved {
            restore(self);
            return Err(error);
        }
        self.write_aliases().await
    }

    /// Returns the keypairs, unlocking the keystore first if it is locked.
    pub fn key_pairs(&self) -> Result<Vec<&SuiKeyPair>, anyhow::Error> {
        Ok(self.keys.private_keys()?.values().collect())
    }
}

/// Serializes keypairs in the format of a plaintext keystore file: a JSON array of Base64 encoded
/// `flag || privkey` strings.
pub(crate) fn encode_key_pairs(
    keys: &BTreeMap<SuiAddress, SuiKeyPair>,
) -> Result<Zeroizing<String>, anyhow::Error> {
    let encoded: Vec<Zeroizing<String>> = keys
        .values()
        .map(|key| Zeroizing::new(key.encode_base64()))
        .collect();
    let encoded_refs: Vec<&str> = encoded.iter().map(|key| key.as_str()).collect();
    // A buffer that is large enough from the start does not leave copies of the keys in memory
    // that it frees as it grows.
    let capacity = encoded.iter().map(|key| key.len() + 8).sum::<usize>() + 4;
    let mut buffer = Zeroizing::new(Vec::with_capacity(capacity));
    serde_json::to_writer_pretty(&mut *buffer, &encoded_refs)?;
    Ok(Zeroizing::new(String::from_utf8(std::mem::take(
        &mut *buffer,
    ))?))
}

/// Decodes Base64 encoded `flag || privkey` strings into keypairs, indexed by address.
pub(crate) fn decode_key_pairs<S: AsRef<str>>(
    encoded: &[S],
) -> Result<BTreeMap<SuiAddress, SuiKeyPair>, anyhow::Error> {
    encoded
        .iter()
        .map(|key| {
            let key = SuiKeyPair::decode_base64(key.as_ref())?;
            Ok((SuiAddress::from(&key.public()), key))
        })
        .collect()
}

/// Returns whether `a` and `b` are the same file, also if they are different paths to it.
fn is_same_file(a: &Path, b: &Path) -> bool {
    a == b || matches!((fs::canonicalize(a), fs::canonicalize(b)), (Ok(a), Ok(b)) if a == b)
}

/// Writes `contents` to the keystore file at `path` in place, and on Unix makes the file readable
/// only by its owner.
async fn write_keystore_file(
    path: &Path,
    contents: impl AsRef<[u8]> + Send + 'static,
    sync: bool,
) -> Result<(), anyhow::Error> {
    use std::io::Write as _;

    let keystore_path = path.to_path_buf();
    // no reactor for tokio::fs::write in simtest, so we use spawn_blocking
    tokio::task::spawn_blocking(move || {
        let ret = fs::File::create(&keystore_path).and_then(|mut file| {
            file.write_all(contents.as_ref())?;
            if sync { file.sync_all() } else { Ok(()) }
        });
        #[cfg(unix)]
        if ret.is_ok() {
            let _ = set_reduced_file_permissions(&keystore_path).inspect_err(|error| {
                eprintln!(
                    "While attempting to set reduced file permissions on '{}'. Cannot set \
                        permissions for keystore file. Error: {error}",
                    keystore_path.display()
                );
            });
        }
        ret
    })
    .await?
    .with_context(|| format!("Cannot write keystore to file: {}", path.display()))
}

/// Writes `contents` to a new file at `path` that only its owner can read on Unix, and returns
/// once the contents and the directory entry of the file are on the disk.
async fn write_file_durably(
    path: &Path,
    contents: impl AsRef<[u8]> + Send + 'static,
) -> std::io::Result<()> {
    use std::io::Write as _;

    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let mut options = fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let mut file = options.open(&path)?;
        // The mode applies only to a new file, and a recovery copy can remain from a crash.
        #[cfg(unix)]
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
        file.write_all(contents.as_ref())?;
        file.sync_all()?;
        // A directory cannot be opened as a file on every platform, so this flush is best effort.
        let directory = path
            .parent()
            .filter(|directory| !directory.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let _ = fs::File::open(directory).and_then(|directory| directory.sync_all());
        Ok(())
    })
    .await?
}

/// Removes the recovery copy at `path` of a keystore that no longer needs it.
fn remove_recovery_copy(path: &Path) {
    if let Err(error) = fs::remove_file(path) {
        eprintln!(
            "Cannot remove the recovery copy of the keystore at {}: {error}",
            path.display()
        );
    }
}

/// Derives a key from `password` with a new salt on a blocking thread, so that the key derivation
/// does not block the async runtime.
async fn generate_key_in_background(
    password: &str,
    cost: Argon2Cost,
) -> Result<KeystoreKey, anyhow::Error> {
    let password = Zeroizing::new(password.to_string());
    tokio::task::spawn_blocking(move || KeystoreKey::generate(&password, cost)).await?
}

/// Parses the contents of the keystore file at `path`, in either format.
fn parse_keystore_file(path: &Path, contents: &[u8]) -> Result<StoredKeys, anyhow::Error> {
    if encryption::is_encrypted_file(contents) {
        let (file, public_keys) = EncryptedKeystoreFile::from_slice(contents)
            .with_context(|| format!("Invalid encrypted keystore file: {}", path.display()))?;
        Ok(StoredKeys::Encrypted(Box::new(EncryptedKeys::new(
            path,
            file,
            public_keys,
        ))))
    } else {
        let kp_strings: Vec<Zeroizing<String>> = serde_json::from_slice(contents)
            .with_context(|| format!("Cannot deserialize the keystore file: {}", path.display()))?;
        Ok(StoredKeys::Plaintext(
            decode_key_pairs(&kp_strings)
                .map_err(|e| anyhow!("Invalid keystore file: {}. {}", path.display(), e))?,
        ))
    }
}

/// Returns the path of the aliases file of the keystore file at `path`.
fn aliases_path(path: &Path) -> PathBuf {
    path.with_extension(ALIASES_FILE_EXTENSION)
}

/// Returns the path of the file next to the keystore file at `path` that holds the new keystore
/// while `sui keytool encrypt`, `decrypt` or `change-password` rewrites the keystore file.
pub fn recovery_copy_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".new");
    path.with_file_name(name)
}

#[cfg(unix)]
fn set_reduced_file_permissions(path: impl AsRef<Path>) -> Result<(), anyhow::Error> {
    let path = path.as_ref();
    let metadata = fs::metadata(path)?;
    let mode = metadata.permissions().mode();
    if mode & 0o177 != 0 {
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).with_context(|| {
            format!(
                "Cannot set permissions for keystore file: {}.",
                path.display(),
            )
        })?;
    }
    Ok(())
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
            .zip_debug_eq(random_names(HashSet::new(), keys.len()))
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
