// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// service health related utilities
///
use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ServiceStatus {
    Up,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct HealthResponse {
    name: String,
    version: String,
    status: ServiceStatus,
}

impl HealthResponse {
    pub fn new(package_name: &str, package_version: &str) -> Self {
        Self {
            name: package_name.to_owned(),
            version: package_version.to_owned(),
            status: ServiceStatus::Up,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_response_new_works() {
        let result = HealthResponse::new("myslib", "0.0.1");
        assert_eq!(
            result,
            HealthResponse {
                name: "myslib".to_owned(),
                version: "0.0.1".to_owned(),
                status: ServiceStatus::Up
            }
        );
    }
}
