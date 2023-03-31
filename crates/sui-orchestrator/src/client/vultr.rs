// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{fmt::Display, net::Ipv4Addr};

use reqwest::{Client as NetworkClient, Response, Url};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    error::{CloudProviderError, CloudProviderResult},
    settings::Settings,
};

use super::{Instance, ServerProviderClient};

/// Make a network error.
impl From<reqwest::Error> for CloudProviderError {
    fn from(e: reqwest::Error) -> Self {
        Self::RequestError(e.to_string())
    }
}

/// Make an error signaling the client provider returned an unexpected response.
impl From<serde_json::Error> for CloudProviderError {
    fn from(e: serde_json::Error) -> Self {
        Self::UnexpectedResponse(e.to_string())
    }
}

/// Represents the ssh key information as defined by Vultr.
#[derive(Debug, Deserialize)]
pub struct SshKey {
    pub id: String,
    pub name: String,
}

/// Represents an instance as defined by Vultr.
#[derive(Debug, Deserialize)]
pub struct VultrInstance {
    pub id: String,
    pub region: String,
    pub main_ip: Ipv4Addr,
    pub tags: Vec<String>,
    pub plan: String,
    pub power_status: String,
}

impl From<VultrInstance> for Instance {
    fn from(instance: VultrInstance) -> Self {
        Self {
            id: instance.id,
            region: instance.region,
            main_ip: instance.main_ip,
            tags: instance.tags,
            specs: instance.plan,
            status: instance.power_status,
        }
    }
}

impl VultrInstance {
    /// Return whether the instance matches the parameters specified in the setting file.
    pub fn filter(&self, settings: &Settings) -> bool {
        settings.regions.contains(&self.region)
            && self.tags.contains(&settings.testbed_id)
            && self.plan == settings.specs
    }
}

/// A Vultr client.
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

    /// Make a new Vultr client.
    pub fn new<T: Into<String>>(token: T, settings: Settings) -> Self {
        Self {
            token: token.into(),
            settings,
            base_url: Self::BASE_URL.parse().unwrap(),
            client: NetworkClient::new(),
        }
    }

    /// Check an http response code.
    fn check_status_code(response: &Response) -> CloudProviderResult<()> {
        if !response.status().is_success() {
            return Err(CloudProviderError::FailureResponseCode(
                response.status().to_string(),
                "[no body]".into(),
            ));
        }
        Ok(())
    }

    /// Check an http response and deduced whether it contains an error.
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

    /// Retrieve the ssh key associated with the current testbed.
    pub async fn get_key(&self) -> CloudProviderResult<Option<SshKey>> {
        let url = self.base_url.join("ssh-keys").unwrap();
        let response = self.client.get(url).bearer_auth(&self.token).send().await?;

        let json: Value = response.json().await?;
        Self::check_response(&json)?;
        let content = json["ssh_keys"].clone();
        let keys: Vec<SshKey> = serde_json::from_value(content)?;

        Ok(keys
            .into_iter()
            .find(|x| x.name == self.settings.testbed_id))
    }

    /// Delete all copies of the public key.
    #[allow(dead_code)]
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

#[async_trait::async_trait]
impl ServerProviderClient for VultrClient {
    const USERNAME: &'static str = "root";

    async fn list_instances(&self) -> CloudProviderResult<Vec<Instance>> {
        let url = self.base_url.join("instances").unwrap();
        let response = self.client.get(url).bearer_auth(&self.token).send().await?;

        let json: Value = response.json().await?;
        Self::check_response(&json)?;
        let content = json["instances"].clone();
        let instances: Vec<VultrInstance> = serde_json::from_value(content)?;

        let filtered = instances
            .into_iter()
            .filter(|x| x.filter(&self.settings))
            .map(|x| x.into())
            .collect();

        Ok(filtered)
    }

    async fn start_instances<'a, I>(&self, instances: I) -> CloudProviderResult<()>
    where
        I: Iterator<Item = &'a Instance> + Send,
    {
        let instance_ids: Vec<_> = instances.map(|x| x.id.clone()).collect();
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

    async fn stop_instances<'a, I>(&self, instances: I) -> CloudProviderResult<()>
    where
        I: Iterator<Item = &'a Instance> + Send,
    {
        let instance_ids: Vec<_> = instances.map(|x| x.id.clone()).collect();
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
        let testbed_name = self.settings.testbed_id.clone();
        let ssh_key_id = match self.get_key().await? {
            Some(key) => key.id,
            None => return Err(CloudProviderError::SshKeyNotFound(testbed_name.clone())),
        };

        let url = self.base_url.join("instances").unwrap();
        let parameters = json!({
                "region": region,
                "plan": self.settings.specs.clone(),
                "os_id": Self::DEFAULT_OS,
                "label": self.settings.testbed_id.clone(),
                "sshkey_id": [ssh_key_id],
                "hostname": "validator",
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

    async fn delete_instance(&self, instance: Instance) -> CloudProviderResult<()> {
        let url = self
            .base_url
            .join(&format!("instances/{}", &instance.id))
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

    async fn register_ssh_public_key(&self, public_key: String) -> CloudProviderResult<()> {
        // Do not upload the key if it already exists.
        if self.get_key().await?.is_some() {
            return Ok(());
        }

        let url = self.base_url.join("ssh-keys").unwrap();
        let parameters = json!({
                "name": self.settings.testbed_id.clone(),
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
        Ok(())
    }

    async fn instance_setup_commands(&self) -> CloudProviderResult<Vec<String>> {
        Ok(vec!["sudo ufw disable".into()])
    }
}
