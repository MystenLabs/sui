use std::fmt::{Debug, Display, Formatter};
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::keytool::Key;
use anyhow::anyhow;
use clap::*;
use json_to_table::{Orientation, json_to_table};
use serde::Serialize;
use serde_json::json;
use sui_keys::external::External;
use sui_keys::keystore::{AccountKeystore, GenerateOptions, GeneratedKey, Keystore};
use tracing::info;

#[allow(clippy::large_enum_variant)]
#[derive(Subcommand)]
#[clap(rename_all = "kebab-case")]
pub enum ExternalKeysCommand {
    /// Generate a new key for an external signer
    Generate { signer: String },
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
            ExternalKeysCommand::Generate { signer } => {
                let Some(external_keys) = external_keys else {
                    return Err(anyhow!("Keystore is not configured for external signer"));
                };
                let Keystore::External(external_keys) = external_keys else {
                    return Err(anyhow!("Keystore is not configured for external signer"));
                };
                let GeneratedKey { public_key, .. } = external_keys
                    .generate(None, GenerateOptions::ExternalSigner(signer))
                    .await?;
                let key = Key::from(public_key);
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
