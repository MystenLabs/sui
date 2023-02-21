use std::fmt::Display;

use reqwest::{Client as NetworkClient, Response, Url};
use serde::Serialize;
use serde_json::{json, Value};

use crate::orchestrator::{
    error::{CloudProviderError, CloudProviderResult},
    settings::Settings,
    state::{Instance, SshKey},
};

#[async_trait::async_trait]
pub trait Client: Display {
    /// The username used to connect to the instances.
    fn username(&self) -> &str;

    /// List all existing instances (regardless of their status).
    async fn list_instances(&self) -> CloudProviderResult<Vec<Instance>>;

    /// Start the specified instances.
    async fn start_instances(&self, instance_ids: Vec<String>) -> CloudProviderResult<()>;

    /// Halt/Stop the specified instances. We may still be billed for stopped instances.
    async fn halt_instances(&self, instance_ids: Vec<String>) -> CloudProviderResult<()>;

    /// Create an instance in a specific region.
    async fn create_instance<S>(&self, region: S) -> CloudProviderResult<Instance>
    where
        S: Into<String> + Serialize + Send;

    /// Delete a specific instance. Calling this function ensures we are no longer billed for
    /// the specified instance.
    async fn delete_instance(&self, instance_id: String) -> CloudProviderResult<()>;
}

pub struct VultrClient {
    token: String,
    settings: Settings,
    base_url: Url,
    client: NetworkClient,
}

impl Display for VultrClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Vultr API client v2")
    }
}

impl VultrClient {
    const BASE_URL: &str = "https://api.vultr.com/v2/";
    const DEFAULT_OS: u16 = 1743; // Ubuntu 22.04 x64
    const INSTANCE_LABEL: &str = "validator"; // Machine label and hostname

    pub fn new<T: Into<String>>(token: T, settings: Settings) -> Self {
        Self {
            token: token.into(),
            settings,
            base_url: Self::BASE_URL.parse().unwrap(),
            client: NetworkClient::new(),
        }
    }

    fn check_status_code(response: &Response) -> CloudProviderResult<()> {
        if !response.status().is_success() {
            return Err(CloudProviderError::FailureResponseCode(
                response.status().to_string(),
                "[no body]".into(),
            ));
        }
        Ok(())
    }

    fn check_response(response: &Value) -> CloudProviderResult<()> {
        response.get("error").map_or_else(
            || Ok(()),
            |_| {
                let status = response["status"].to_string();
                let message = response["error"].to_string();
                Err(CloudProviderError::FailureResponseCode(status, message))
            },
        )
    }
}

#[async_trait::async_trait]
impl Client for VultrClient {
    fn username(&self) -> &str {
        "root"
    }

    async fn list_instances(&self) -> CloudProviderResult<Vec<Instance>> {
        let url = self.base_url.join("instances").unwrap();
        let response = self.client.get(url).bearer_auth(&self.token).send().await?;

        let json: Value = response.json().await?;
        Self::check_response(&json)?;
        let content = json["instances"].clone();
        // println!("{content:?}");
        let instances: Vec<Instance> = serde_json::from_value(content)?;

        let filtered = instances
            .into_iter()
            .filter(|x| x.filter(&self.settings))
            .collect();

        Ok(filtered)
    }

    async fn start_instances(&self, instance_ids: Vec<String>) -> CloudProviderResult<()> {
        let url = self.base_url.join("instances/start").unwrap();
        let parameters = json!({ "instance_ids": instance_ids });

        let response = self
            .client
            .post(url)
            .bearer_auth(&self.token)
            .json(&parameters)
            .send()
            .await?;

        Self::check_status_code(&response)?;
        Ok(())
    }

    async fn halt_instances(&self, instance_ids: Vec<String>) -> CloudProviderResult<()> {
        let url = self.base_url.join("instances/halt").unwrap();
        let parameters = json!({ "instance_ids": instance_ids });

        let response = self
            .client
            .post(url)
            .bearer_auth(&self.token)
            .json(&parameters)
            .send()
            .await?;

        Self::check_status_code(&response)?;
        Ok(())
    }

    async fn create_instance<S>(&self, region: S) -> CloudProviderResult<Instance>
    where
        S: Into<String> + Serialize + Send,
    {
        let testbed_name = self.settings.testbed.clone();
        let ssh_key_id = match self.get_key().await? {
            Some(key) => key.id,
            None => return Err(CloudProviderError::SshKeyNotFound(testbed_name.clone())),
        };

        let url = self.base_url.join("instances").unwrap();
        let parameters = json!({
                "region": region,
                "plan": self.settings.specs.clone(),
                "os_id": Self::DEFAULT_OS,
                "label": Self::INSTANCE_LABEL,
                "sshkey_id": [ssh_key_id],
                "hostname": Self::INSTANCE_LABEL,
                "tag": testbed_name
        });

        let response = self
            .client
            .post(url)
            .bearer_auth(&self.token)
            .json(&parameters)
            .send()
            .await?;

        let json: Value = response.json().await?;
        Self::check_response(&json)?;
        let content = json["instance"].clone();
        serde_json::from_value::<Instance>(content).map_err(CloudProviderError::from)
    }

    async fn delete_instance(&self, instance_id: String) -> CloudProviderResult<()> {
        let url = self
            .base_url
            .join(&format!("instances/{}", instance_id))
            .unwrap();

        let response = self
            .client
            .delete(url)
            .bearer_auth(&self.token)
            .send()
            .await?;

        Self::check_status_code(&response)?;
        Ok(())
    }
}

impl VultrClient {
    /// Retrieve the ssh key associated with the current testbed.
    pub async fn get_key(&self) -> CloudProviderResult<Option<SshKey>> {
        let url = self.base_url.join("ssh-keys").unwrap();
        let response = self.client.get(url).bearer_auth(&self.token).send().await?;

        let json: Value = response.json().await?;
        Self::check_response(&json)?;
        let content = json["ssh_keys"].clone();
        let keys: Vec<SshKey> = serde_json::from_value(content)?;

        Ok(keys.into_iter().find(|x| x.name == self.settings.testbed))
    }

    /// Upload an ssh public key if there isn't already one.
    pub async fn upload_key(&self, public_key: String) -> CloudProviderResult<SshKey> {
        // Do not upload the key if it already exists.
        if let Some(key) = self.get_key().await? {
            return Ok(key);
        }

        let url = self.base_url.join("ssh-keys").unwrap();
        let parameters = json!({
                "name": self.settings.testbed.clone(),
                "ssh_key": public_key
        });

        let response = self
            .client
            .post(url)
            .bearer_auth(&self.token)
            .json(&parameters)
            .send()
            .await?;

        let json: Value = response.json().await?;
        Self::check_response(&json)?;
        let content = json["ssh_key"].clone();
        serde_json::from_value::<SshKey>(content).map_err(CloudProviderError::from)
    }

    /// Delete all copies of the public key.
    pub async fn remove_key(&self) -> CloudProviderResult<()> {
        while let Some(key) = self.get_key().await? {
            let url = self.base_url.join(&format!("ssh-keys/{}", key.id)).unwrap();

            let response = self
                .client
                .delete(url)
                .bearer_auth(&self.token)
                .send()
                .await?;

            let json: Value = response.json().await?;
            Self::check_response(&json)?;
        }
        Ok(())
    }
}
