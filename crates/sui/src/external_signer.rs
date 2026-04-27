// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::keytool::Key;
use anyhow::anyhow;
use clap::*;
use json_to_table::{Orientation, json_to_table};
use serde::Serialize;
use serde_json::json;
use std::fmt::{Debug, Display, Formatter};
use sui_keys::external::{External, ProvisionMode};
use sui_keys::keystore::{AccountKeystore, GenerateOptions, GeneratedKey, Keystore};
use tracing::info;

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[clap(rename_all = "kebab-case")]
pub enum ProvisionModeArg {
    RecoverableAssumed,
    MnemonicBacked,
    NonRecoverable,
}

impl From<ProvisionModeArg> for ProvisionMode {
    fn from(value: ProvisionModeArg) -> Self {
        match value {
            ProvisionModeArg::RecoverableAssumed => ProvisionMode::RecoverableAssumed,
            ProvisionModeArg::MnemonicBacked => ProvisionMode::MnemonicBacked,
            ProvisionModeArg::NonRecoverable => ProvisionMode::NonRecoverable,
        }
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Subcommand)]
#[clap(rename_all = "kebab-case")]
pub enum ExternalKeysCommand {
    /// Generate a new key for an external signer
    Generate {
        signer: String,
        /// Provisioning mode to request when creating a new signer key.
        #[clap(long, value_enum)]
        provision_mode: Option<ProvisionModeArg>,
    },
    /// List all keys available for an external signer
    ListKeys { signer: String },
    /// Add an existing key to the sui cli for an existing external signer key
    AddExisting { key_id: String, signer: String },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalKey {
    key_id: String,
    is_indexed: bool,
    key: Key,
}

#[derive(Serialize)]
#[serde(untagged)]
pub enum CommandOutput {
    ExternalGenerate(Key),
    ExternalList(Vec<ExternalKey>),
    ExternalAddExisting(ExternalKey),
    Error(String),
}

impl ExternalKeysCommand {
    pub async fn execute(
        self,
        external_keys: Option<&mut Keystore>,
    ) -> Result<CommandOutput, anyhow::Error> {
        match self {
            ExternalKeysCommand::Generate {
                signer,
                provision_mode,
            } => {
                let Some(external_keys) = external_keys else {
                    return Err(anyhow!("Keystore is not configured for external signer"));
                };
                let Keystore::External(external_keys) = external_keys else {
                    return Err(anyhow!("Keystore is not configured for external signer"));
                };
                let GeneratedKey {
                    public_key,
                    mnemonic,
                    ..
                } = external_keys
                    .generate(
                        None,
                        GenerateOptions::ExternalSigner {
                            signer,
                            provision_mode: provision_mode.map(Into::into),
                        },
                    )
                    .await?;
                let key = Key::from(public_key).with_mnemonic(mnemonic);
                external_keys.save().await?;
                Ok(CommandOutput::ExternalGenerate(key))
            }
            ExternalKeysCommand::ListKeys { signer } => {
                let external_keys = get_external_keystore(external_keys)?;
                let keys = external_keys.signer_available_keys(signer.clone()).await?;
                let keys: Vec<ExternalKey> = keys
                    .into_iter()
                    .map(|key| ExternalKey {
                        is_indexed: external_keys.is_indexed(&key),
                        key_id: key.key_id,
                        key: Key::from(key.public_key),
                    })
                    .collect::<Vec<ExternalKey>>();
                Ok(CommandOutput::ExternalList(keys))
            }
            ExternalKeysCommand::AddExisting { key_id, signer } => {
                let external_keys = get_external_keystore(external_keys)?;
                let key = external_keys.add_existing(signer.clone(), key_id).await?;
                Ok(CommandOutput::ExternalAddExisting(ExternalKey {
                    is_indexed: true,
                    key_id: key.key_id,
                    key: Key::from(key.public_key),
                }))
            }
        }
    }
}

impl Display for CommandOutput {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        let json_obj = json![self];
        let mut table = json_to_table(&json_obj);
        let style = tabled::settings::Style::rounded().horizontals([]);
        table.with(style);
        table.array_orientation(Orientation::Column);
        write!(formatter, "{}", table)
    }
}

impl CommandOutput {
    pub fn print(&self, pretty: bool) {
        let line = if pretty {
            format!("{self}")
        } else {
            format!("{:?}", self)
        };
        // Log line by line
        for line in line.lines() {
            // Logs write to a file on the side.  Print to stdout and also log to file, for tests to pass.
            println!("{line}");
            info!("{line}")
        }
    }
}

// when --json flag is used, any output result is transformed into a JSON pretty string and sent to std output
impl Debug for CommandOutput {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match serde_json::to_string_pretty(self) {
            Ok(json) => write!(f, "{json}"),
            Err(err) => write!(f, "Error serializing JSON: {err}"),
        }
    }
}

// unwrap the keystore Option<Keystore> => External
fn get_external_keystore(keystore: Option<&mut Keystore>) -> Result<&mut External, anyhow::Error> {
    let Some(keystore) = keystore else {
        return Err(anyhow!("Keystore is not configured for external signer"));
    };
    let Keystore::External(external_keystore) = keystore else {
        return Err(anyhow!("Keystore is not configured for external signer"));
    };
    Ok(external_keystore)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use anyhow::Error;
    use async_trait::async_trait;
    use clap::Parser;
    use serde_json::{Value, json};
    use sui_keys::external::{CommandRunner, ExternalExecError};
    use sui_keys::keystore::Keystore;
    use tempfile::TempDir;

    use super::{CommandOutput, ExternalKeysCommand, ProvisionModeArg};
    use crate::sui_commands::SuiCommand;
    use sui_keys::external::External;

    #[derive(Debug)]
    struct StubCommandRunner {
        response: Value,
    }

    #[async_trait]
    impl CommandRunner for StubCommandRunner {
        async fn run(
            &self,
            command: &str,
            method: &str,
            params: Value,
        ) -> Result<Value, ExternalExecError> {
            assert_eq!(command, "yubikey");
            assert_eq!(method, "create_key");
            assert_eq!(params, json!({ "mode": "mnemonic-backed" }));
            Ok(self.response.clone())
        }
    }

    #[test]
    fn test_external_keys_generate_parses_provision_mode() {
        let command = SuiCommand::try_parse_from([
            "sui",
            "external-keys",
            "generate",
            "yubikey",
            "--provision-mode",
            "mnemonic-backed",
        ])
        .unwrap();

        let SuiCommand::ExternalKeys { cmd, .. } = command else {
            panic!("expected external-keys command");
        };
        let ExternalKeysCommand::Generate {
            signer,
            provision_mode,
        } = cmd
        else {
            panic!("expected external-keys generate command");
        };

        assert_eq!(signer, "yubikey");
        assert_eq!(provision_mode, Some(ProvisionModeArg::MnemonicBacked));
    }

    #[tokio::test]
    async fn test_external_keys_generate_surfaces_signer_mnemonic() -> Result<(), Error> {
        let tmp_dir = TempDir::new()?;
        let tmp_keystore = PathBuf::from(tmp_dir.path()).join("external.keystore");
        let mut keystore = Keystore::External(External::new_for_test(
            Box::new(StubCommandRunner {
                response: json!({
                    "key_id": "key-123",
                    "public_key": {
                        "Ed25519": "snQZotwFNPBNOHl2/JzrFrHCuOQbWylDOUv5bgIYuoY="
                    },
                    "mnemonic": "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about"
                }),
            }),
            Some(tmp_keystore),
        ));

        let output = ExternalKeysCommand::Generate {
            signer: "yubikey".to_string(),
            provision_mode: Some(ProvisionModeArg::MnemonicBacked),
        }
        .execute(Some(&mut keystore))
        .await?;

        let CommandOutput::ExternalGenerate(key) = output else {
            panic!("expected generated key output");
        };
        let json = serde_json::to_value(key)?;
        assert_eq!(
            json["mnemonic"],
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about"
        );
        Ok(())
    }
}
