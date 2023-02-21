use std::{
    fmt::Display,
    fs::{self},
    path::{Path, PathBuf},
};

use reqwest::Url;
use serde::{de::Error, Deserialize, Deserializer};

use crate::orchestrator::error::{SettingsError, SettingsResult};

#[derive(Deserialize, Clone)]
pub struct Repository {
    pub name: String,
    #[serde(deserialize_with = "parse_url")]
    pub url: Url,
    pub branch: String,
}

fn parse_url<'de, D>(deserializer: D) -> Result<Url, D::Error>
where
    D: Deserializer<'de>,
{
    let s: &str = Deserialize::deserialize(deserializer)?;
    Url::parse(&s).map_err(D::Error::custom)
}

#[derive(Deserialize, Clone)]
pub struct Settings {
    pub testbed: String,
    pub token_file: PathBuf,
    pub ssh_private_key_file: PathBuf,
    pub ssh_public_key_file: Option<PathBuf>,
    pub regions: Vec<String>,
    pub specs: String,
    pub repository: Repository,
}

impl Settings {
    pub fn load<P>(path: P) -> SettingsResult<Self>
    where
        P: AsRef<Path> + Display + Clone,
    {
        let reader = || -> Result<Self, std::io::Error> {
            let data = fs::read(path.clone())?;
            Ok(serde_json::from_slice(data.as_slice())?)
        };
        reader().map_err(|e| SettingsError::InvalidSettings {
            file: path.to_string(),
            message: e.to_string(),
        })
    }

    pub fn load_token(&self) -> SettingsResult<String> {
        match fs::read_to_string(&self.token_file) {
            Ok(token) => Ok(token.trim_end_matches('\n').to_string()),
            Err(e) => Err(SettingsError::InvalidTokenFile {
                file: self.token_file.display().to_string(),
                message: e.to_string(),
            }),
        }
    }

    pub fn load_ssh_public_key(&self) -> SettingsResult<String> {
        let ssh_public_key_file = self.ssh_public_key_file.clone().unwrap_or_else(|| {
            let mut private = self.ssh_private_key_file.clone();
            private.set_extension("pub");
            private
        });
        match fs::read_to_string(&ssh_public_key_file) {
            Ok(token) => Ok(token.trim_end_matches('\n').to_string()),
            Err(e) => Err(SettingsError::InvalidSshPublicKeyFile {
                file: ssh_public_key_file.display().to_string(),
                message: e.to_string(),
            }),
        }
    }

    #[cfg(test)]
    pub fn number_of_regions(&self) -> usize {
        self.regions.len()
    }
}
