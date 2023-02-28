use std::{collections::HashMap, fmt::Display};

use aws_config::profile::profile_file::{ProfileFileKind, ProfileFiles};
use aws_sdk_ec2::{model::filter, types::SdkError, Region};
use serde::Serialize;

use crate::orchestrator::{
    error::{CloudProviderError, CloudProviderResult},
    settings::Settings,
    state::Instance,
};

use super::Client;

impl<T> From<SdkError<T>> for CloudProviderError {
    fn from(e: SdkError<T>) -> Self {
        Self::RequestError(e.to_string())
    }
}

pub struct AwsClient {
    settings: Settings,
    clients: HashMap<Region, aws_sdk_ec2::Client>,
}

impl Display for AwsClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "AWS EC2 client v{}", aws_sdk_ec2::PKG_VERSION)
    }
}

impl AwsClient {
    pub async fn new(settings: Settings) -> Self {
        let profile_files = ProfileFiles::builder()
            .with_file(ProfileFileKind::Credentials, &settings.token_file)
            .with_contents(
                ProfileFileKind::Config,
                "[default]\nregion=us-west-2\noutput=json",
            )
            .build();

        let mut clients = HashMap::new();
        for region_name in settings.regions.clone() {
            let region = Region::new(region_name);
            let sdk_config = aws_config::from_env()
                .region(region.clone())
                .profile_files(profile_files.clone())
                .load()
                .await;
            let client = aws_sdk_ec2::Client::new(&sdk_config);
            clients.insert(region, client);
        }

        Self { settings, clients }
    }
}

#[async_trait::async_trait]
impl Client for AwsClient {
    const USERNAME: &'static str = "ubuntu";

    async fn list_instances(&self) -> CloudProviderResult<Vec<Instance>> {
        let filter = filter::Builder::default()
            .name("tag:Name")
            .values(self.settings.testbed.clone())
            .build();

        let mut list = Vec::new();
        for (region, client) in &self.clients {
            if let Some(reservations) = client
                .describe_instances()
                .filters(filter.clone())
                .send()
                .await?
                .reservations()
            {
                for reservation in reservations {
                    if let Some(instances) = reservation.instances() {
                        for instance in instances {
                            let x = Instance {
                                id: instance.instance_id().unwrap().into(),
                                region: region.to_string(),
                                main_ip: instance.public_ip_address().unwrap().parse().unwrap(),
                                tags: vec![self.settings.testbed.clone()],
                                plan: format!("{:?}", instance.instance_type().unwrap()),
                                power_status: format!(
                                    "{:?}",
                                    instance.state().unwrap().name().unwrap()
                                ),
                            };
                            list.push(x);
                        }
                    }
                }
            }
        }

        Ok(list)
    }

    async fn start_instances(&self, _instance_ids: Vec<String>) -> CloudProviderResult<()> {
        todo!()
    }

    async fn halt_instances(&self, _instance_ids: Vec<String>) -> CloudProviderResult<()> {
        todo!()
    }

    async fn create_instance<S>(&self, _region: S) -> CloudProviderResult<Instance>
    where
        S: Into<String> + Serialize + Send,
    {
        todo!()
    }

    async fn delete_instance(&self, _instance_id: String) -> CloudProviderResult<()> {
        todo!()
    }
}

// #[cfg(test)]
// mod test {
//     use crate::orchestrator::{
//         client::{aws::AwsClient, Client},
//         settings::Settings,
//     };

//     #[tokio::test]
//     async fn aws() {
//         let mut settings = Settings::new_for_test();
//         settings.testbed = "alberto-sui".into();
//         settings.token_file = "/Users/alberto/.aws/credentials".into();
//         settings.regions = vec!["us-east-1".into(), "us-west-2".into()];
//         // g5.8xlarge
//         let client = AwsClient::new(settings).await;
//         client.list_instances().await.unwrap();
//     }
// }
