use std::{
    fmt::Display,
    net::{Ipv4Addr, SocketAddr},
};

use serde::{Deserialize, Serialize};

use super::error::CloudProviderResult;

pub mod aws;
pub mod vultr;

#[derive(Debug, Deserialize, Clone, Eq, PartialEq, Hash)]
pub struct Instance {
    pub id: String,
    pub region: String,
    pub main_ip: Ipv4Addr,
    pub tags: Vec<String>,
    pub plan: String,
    pub power_status: String,
}

impl Instance {
    pub fn is_active(&self) -> bool {
        self.power_status.to_lowercase() == "running"
    }

    pub fn is_inactive(&self) -> bool {
        !self.is_active()
    }

    pub fn is_terminated(&self) -> bool {
        self.power_status.to_lowercase() == "terminated"
    }

    pub fn ssh_address(&self) -> SocketAddr {
        format!("{}:22", self.main_ip).parse().unwrap()
    }
}

#[async_trait::async_trait]
pub trait Client: Display {
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
}

#[cfg(test)]
pub mod test_client {
    use std::{fmt::Display, sync::Mutex};

    use serde::Serialize;

    use crate::error::CloudProviderResult;

    use super::{Client, Instance};

    #[derive(Default)]
    pub struct TestClient {
        instances: Mutex<Vec<Instance>>,
    }

    impl Display for TestClient {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "TestClient")
        }
    }

    #[async_trait::async_trait]
    impl Client for TestClient {
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
                instance.power_status = "running".into();
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
                instance.power_status = "stopped".into();
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
                plan: "".into(),
                power_status: "running".into(),
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
    }
}
