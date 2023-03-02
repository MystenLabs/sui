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
    #[serde(deserialize_with = "parse_url")]
    pub url: Url,
    pub branch: String,
}

fn parse_url<'de, D>(deserializer: D) -> Result<Url, D::Error>
where
    D: Deserializer<'de>,
{
    let s: &str = Deserialize::deserialize(deserializer)?;
    let url = Url::parse(&s).map_err(D::Error::custom)?;

    match url
        .path_segments()
        .map(|x| x.collect::<Vec<_>>().len() >= 2)
    {
        None | Some(false) => Err(D::Error::custom(SettingsError::InvalidRepositoryUrl(
            url.clone(),
        ))),
        _ => Ok(url),
    }
}

#[derive(Deserialize, Clone)]
pub enum CloudProvider {
    #[serde(alias = "aws")]
    Aws,
    #[serde(alias = "vultr")]
    Vultr,
}

#[derive(Deserialize, Clone)]
pub struct Settings {
    pub testbed_id: String,
    pub cloud_provider: CloudProvider,
    pub token_file: PathBuf,
    pub ssh_private_key_file: PathBuf,
    pub ssh_public_key_file: Option<PathBuf>,
    pub regions: Vec<String>,
    pub specs: String,
    pub repository: Repository,
    pub results_directory: PathBuf,
    pub logs_directory: PathBuf,
}

impl Settings {
    pub fn load<P>(path: P) -> SettingsResult<Self>
    where
        P: AsRef<Path> + Display + Clone,
    {
        let reader = || -> Result<Self, std::io::Error> {
            let data = fs::read(path.clone())?;
            let settings: Settings = serde_json::from_slice(data.as_slice())?;

            fs::create_dir_all(&settings.results_directory)?;
            fs::create_dir_all(&settings.logs_directory)?;

            Ok(settings)
        };

        reader().map_err(|e| SettingsError::InvalidSettings {
            file: path.to_string(),
            message: e.to_string(),
        })
    }

    pub fn repository_name(&self) -> String {
        self.repository
            .url
            .path_segments()
            .expect("Url should already be checked when loading settings")
            .collect::<Vec<_>>()[1]
            .to_string()
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

    /// Test settings for unit tests.
    #[cfg(test)]
    pub fn new_for_test() -> Self {
        Self {
            testbed_id: "testbed".into(),
            cloud_provider: CloudProvider::Aws,
            token_file: "/path/to/token/file".into(),
            ssh_private_key_file: "/path/to/private/key/file".into(),
            ssh_public_key_file: None,
            regions: vec!["London".into(), "New York".into()],
            specs: "small".into(),
            repository: Repository {
                url: Url::parse("https://example.net/my-repo").unwrap(),
                branch: "main".into(),
            },
            results_directory: "results".into(),
            logs_directory: "logs".into(),
        }
    }
}
