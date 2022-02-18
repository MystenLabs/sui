// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::fs;
use std::fs::File;
use std::io::BufReader;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use sui_types::base_types::{decode_bytes_hex, encode_bytes_hex, SuiAddress};
use tracing::log::trace;

pub const DEFAULT_STARTING_PORT: u16 = 10000;

pub trait Config
where
    Self: DeserializeOwned + Serialize,
{
    fn read_or_create(path: &Path) -> Result<Self, anyhow::Error> {
        let path_buf = PathBuf::from(path);
        Ok(if path_buf.exists() {
            Self::read(path)?
        } else {
            trace!("Config file not found, creating new config '{:?}'", path);
            let new_config = Self::create(path)?;
            new_config.write(path)?;
            new_config
        })
    }

    fn read(path: &Path) -> Result<Self, anyhow::Error> {
        trace!("Reading config from '{:?}'", path);
        let reader = BufReader::new(File::open(path)?);
        let mut config: Self = serde_json::from_reader(reader)?;
        config.set_config_path(path);
        Ok(config)
    }

    fn write(&self, path: &Path) -> Result<(), anyhow::Error> {
        trace!("Writing config to '{:?}'", path);
        let config = serde_json::to_string_pretty(self).unwrap();
        fs::write(path, config).expect("Unable to write to config file");
        Ok(())
    }

    fn save(&self) -> Result<(), anyhow::Error> {
        self.write(self.config_path())
    }

    fn create(path: &Path) -> Result<Self, anyhow::Error>;

    fn set_config_path(&mut self, path: &Path);
    fn config_path(&self) -> &Path;
}

pub struct PortAllocator {
    next_port: u16,
}

impl PortAllocator {
    pub fn new(starting_port: u16) -> Self {
        Self {
            next_port: starting_port,
        }
    }
    pub fn next_port(&mut self) -> Option<u16> {
        for port in self.next_port..65535 {
            if TcpListener::bind(("127.0.0.1", port)).is_ok() {
                self.next_port = port + 1;
                return Some(port);
            }
        }
        None
    }
}

pub fn optional_address_as_hex<S>(
    key: &Option<SuiAddress>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::ser::Serializer,
{
    serializer.serialize_str(
        &*key
            .map(|addr| encode_bytes_hex(&addr))
            .unwrap_or_else(|| "".to_string()),
    )
}

pub fn optional_address_from_hex<'de, D>(deserializer: D) -> Result<Option<SuiAddress>, D::Error>
where
    D: serde::de::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    let value = decode_bytes_hex(&s).map_err(serde::de::Error::custom)?;
    Ok(Some(value))
}
