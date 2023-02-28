use std::net::{Ipv4Addr, SocketAddr};

use serde::Deserialize;

use crate::orchestrator::settings::Settings;

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

    pub fn filter(&self, settings: &Settings) -> bool {
        settings.regions.contains(&self.region)
            && self.tags.contains(&settings.testbed)
            && self.plan == settings.specs
    }

    pub fn ssh_address(&self) -> SocketAddr {
        format!("{}:22", self.main_ip).parse().unwrap()
    }
}

#[derive(Debug, Deserialize)]
pub struct SshKey {
    pub id: String,
    pub name: String,
    // ssh_key: String,
}
