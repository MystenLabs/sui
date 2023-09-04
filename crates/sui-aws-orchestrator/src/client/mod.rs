// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    fmt::Display,
    net::{Ipv4Addr, SocketAddr},
};

use serde::{Deserialize, Serialize};

use super::error::CloudProviderResult;

pub mod aws;

/// Represents a cloud provider instance.
#[derive(Debug, Deserialize, Clone, Eq, PartialEq, Hash)]
pub struct Instance {
    /// The unique identifier of the instance.
    pub id: String,
    /// The region where the instance runs.
    pub region: String,
    /// The public ip address of the instance (accessible from anywhere).
    pub main_ip: Ipv4Addr,
    /// The list of tags associated with the instance.
    pub tags: Vec<String>,
    /// The specs of the instance.
    pub specs: String,
    /// The current status of the instance.
    pub status: String,
}

impl Instance {
    /// Return whether the instance is active and running.
    pub fn is_active(&self) -> bool {
        self.status.to_lowercase() == "running"
    }

    /// Return whether the instance is inactive and not ready for use.
    pub fn is_inactive(&self) -> bool {
        !self.is_active()
    }

    /// Return whether the instance is terminated and in the process of being deleted.
    pub fn is_terminated(&self) -> bool {
        self.status.to_lowercase() == "terminated"
    }

    /// Return the ssh address to connect to the instance.
    pub fn ssh_address(&self) -> SocketAddr {
        format!("{}:22", self.main_ip).parse().unwrap()
    }

    #[cfg(test)]
    pub fn new_for_test(id: String) -> Self {
        Self {
            id,
            region: Default::default(),
            main_ip: Ipv4Addr::new(127, 0, 0, 1),
            tags: Default::default(),
            specs: Default::default(),
            status: Default::default(),
        }
    }
}

#[async_trait::async_trait]
pub trait ServerProviderClient: Display {
    /// The username used to connect to the instances.
    const USERNAME: &'static str;

    /// List all existing instances (regardless of their status).
    async fn list_instances(&self) -> CloudProviderResult<Vec<Instance>>;

    /// Start the specified instances.
    async fn start_instances<'a, I>(&self, instances: I) -> CloudProviderResult<()>
    where
        I: Iterator<Item = &'a Instance> + Send;

    /// Halt/Stop the specified instances. We may still be billed for stopped instances.
    async fn stop_instances<'a, I>(&self, instance_ids: I) -> CloudProviderResult<()>
    where
        I: Iterator<Item = &'a Instance> + Send;

    /// Create an instance in a specific region.
    async fn create_instance<S>(&self, region: S) -> CloudProviderResult<Instance>
    where
        S: Into<String> + Serialize + Send;

    /// Delete a specific instance. Calling this function ensures we are no longer billed for
    /// the specified instance.
    async fn delete_instance(&self, instance: Instance) -> CloudProviderResult<()>;

    /// Authorize the provided ssh public key to access machines.
    async fn register_ssh_public_key(&self, public_key: String) -> CloudProviderResult<()>;

    /// Return provider-specific commands to setup the instance.
    async fn instance_setup_commands(&self) -> CloudProviderResult<Vec<String>>;
}

#[cfg(test)]
pub mod test_client {
    use std::{fmt::Display, sync::Mutex};

    use serde::Serialize;

    use crate::{error::CloudProviderResult, settings::Settings};

    use super::{Instance, ServerProviderClient};

    pub struct TestClient {
        settings: Settings,
        instances: Mutex<Vec<Instance>>,
    }

    impl TestClient {
        pub fn new(settings: Settings) -> Self {
            Self {
                settings,
                instances: Mutex::new(Vec::new()),
            }
        }
    }

    impl Display for TestClient {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "TestClient")
        }
    }

    #[async_trait::async_trait]
    impl ServerProviderClient for TestClient {
        const USERNAME: &'static str = "root";

        async fn list_instances(&self) -> CloudProviderResult<Vec<Instance>> {
            let guard = self.instances.lock().unwrap();
            Ok(guard.clone())
        }

        async fn start_instances<'a, I>(&self, instances: I) -> CloudProviderResult<()>
        where
            I: Iterator<Item = &'a Instance> + Send,
        {
            let instance_ids: Vec<_> = instances.map(|x| x.id.clone()).collect();
            let mut guard = self.instances.lock().unwrap();
            for instance in guard.iter_mut().filter(|x| instance_ids.contains(&x.id)) {
                instance.status = "running".into();
            }
            Ok(())
        }

        async fn stop_instances<'a, I>(&self, instances: I) -> CloudProviderResult<()>
        where
            I: Iterator<Item = &'a Instance> + Send,
        {
            let instance_ids: Vec<_> = instances.map(|x| x.id.clone()).collect();
            let mut guard = self.instances.lock().unwrap();
            for instance in guard.iter_mut().filter(|x| instance_ids.contains(&x.id)) {
                instance.status = "stopped".into();
            }
            Ok(())
        }

        async fn create_instance<S>(&self, region: S) -> CloudProviderResult<Instance>
        where
            S: Into<String> + Serialize + Send,
        {
            let mut guard = self.instances.lock().unwrap();
            let id = guard.len();
            let instance = Instance {
                id: id.to_string(),
                region: region.into(),
                main_ip: format!("0.0.0.{id}").parse().unwrap(),
                tags: Vec::new(),
                specs: self.settings.specs.clone(),
                status: "running".into(),
            };
            guard.push(instance.clone());
            Ok(instance)
        }

        async fn delete_instance(&self, instance: Instance) -> CloudProviderResult<()> {
            let mut guard = self.instances.lock().unwrap();
            guard.retain(|x| x.id != instance.id);
            Ok(())
        }

        async fn register_ssh_public_key(&self, _public_key: String) -> CloudProviderResult<()> {
            Ok(())
        }

        async fn instance_setup_commands(&self) -> CloudProviderResult<Vec<String>> {
            Ok(Vec::new())
        }
    }
}
