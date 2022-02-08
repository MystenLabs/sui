// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::fs;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use tracing::log::trace;

pub trait Config
where
    Self: DeserializeOwned + Serialize,
{
    fn read_or_create(path: &str) -> Result<Self, anyhow::Error> {
        let path_buf = PathBuf::from(path);
        Ok(if path_buf.exists() {
            trace!("Reading config from '{}'", path);
            let reader = BufReader::new(File::open(path_buf)?);
            let mut config: Self = serde_json::from_reader(reader)?;
            config.set_config_path(path);
            config
        } else {
            trace!("Config file not found, creating new config '{}'", path);
            let new_config = Self::create(path)?;
            new_config.write(path)?;
            new_config
        })
    }

    fn write(&self, path: &str) -> Result<(), anyhow::Error> {
        trace!("Writing config to '{}'", path);
        let config = serde_json::to_string_pretty(self).unwrap();
        fs::write(path, config).expect("Unable to write to config file");
        Ok(())
    }

    fn save(&self) -> Result<(), anyhow::Error> {
        self.write(self.config_path())
    }

    fn create(path: &str) -> Result<Self, anyhow::Error>;

    fn set_config_path(&mut self, path: &str);
    fn config_path(&self) -> &str;
}