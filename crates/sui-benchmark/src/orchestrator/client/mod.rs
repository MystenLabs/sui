use std::fmt::Display;

use serde::Serialize;

use super::{error::CloudProviderResult, state::Instance};

pub mod aws;
pub mod vultr;

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
    async fn delete_instance(&self, instance_id: String) -> CloudProviderResult<()>;
}
