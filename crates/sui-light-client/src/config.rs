// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use sui_config::object_storage_config::ObjectStoreConfig;
use url::Url;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Config {
    pub checkpoint_summary_dir: PathBuf,
    pub full_node_url: String,
    pub object_store_url: String,
    pub archive_store_config: Option<ObjectStoreConfig>,
    pub graphql_url: Option<String>,
    pub genesis_filename: String,
}

impl Config {
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = fs::File::open(path)?;
        let config: Config = serde_yaml::from_reader(file)?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        if !self.checkpoint_summary_dir.is_dir() {
            return Err(anyhow!("Checkpoint summary directory does not exist"));
        }

        Url::parse(&self.full_node_url).map_err(|_| anyhow!("Invalid full node URL"))?;

        Url::parse(&self.object_store_url).map_err(|_| anyhow!("Invalid object store URL"))?;

        if let Some(url) = &self.graphql_url {
            Url::parse(url).map_err(|_| anyhow!("Invalid GraphQL URL"))?;
        }

        Ok(())
    }

    pub fn checkpoint_list_path(&self) -> PathBuf {
        self.checkpoint_summary_dir.join("checkpoints.yaml")
    }

    pub fn checkpoint_path(&self, seq: u64, custom_path: Option<&str>) -> PathBuf {
        let mut path = self.checkpoint_summary_dir.clone();
        if let Some(custom) = custom_path {
            path.push(custom);
        }
        path.push(format!("{}.yaml", seq));
        path
    }

    pub fn genesis_path(&self) -> PathBuf {
        self.checkpoint_summary_dir.join(&self.genesis_filename)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sui_config::object_storage_config::ObjectStoreType;
    use tempfile::TempDir;

    fn create_test_config() -> (Config, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config = Config {
            checkpoint_summary_dir: temp_dir.path().to_path_buf(),
            full_node_url: "http://localhost:9000".to_string(),
            object_store_url: "http://localhost:9001".to_string(),
            archive_store_config: Some(ObjectStoreConfig {
                object_store: Some(ObjectStoreType::File),
                directory: Some(temp_dir.path().to_path_buf()),
                ..Default::default()
            }),
            graphql_url: Some("http://localhost:9003".to_string()),
            genesis_filename: "genesis.blob".to_string(),
        };
        (config, temp_dir)
    }

    #[test]
    fn test_config_validation() {
        let (config, _temp_dir) = create_test_config();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_checkpoint_paths() {
        let (config, _temp_dir) = create_test_config();

        let list_path = config.checkpoint_list_path();
        assert_eq!(list_path.file_name().unwrap(), "checkpoints.yaml");

        let checkpoint_path = config.checkpoint_path(123, None);
        assert_eq!(checkpoint_path.file_name().unwrap(), "123.yaml");

        let custom_checkpoint_path = config.checkpoint_path(456, Some("custom"));
        assert!(custom_checkpoint_path.to_str().unwrap().contains("custom"));
        assert_eq!(custom_checkpoint_path.file_name().unwrap(), "456.yaml");
    }

    #[test]
    fn test_genesis_path() {
        let (config, _temp_dir) = create_test_config();
        let genesis_path = config.genesis_path();
        assert_eq!(genesis_path.file_name().unwrap(), "genesis.blob");
    }
}
