// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result};
use k8s_openapi::api::core::v1::Pod;
use kube::{
    api::{Api, LogParams},
    config::Kubeconfig,
    Client, Config,
};
use tracing::debug;

use crate::{cache_local, get_cached_local, run_cmd};

fn get_kubeconfig_key(stack: &str) -> String {
    format!("kubeconfig.{}.yaml", stack)
}

pub async fn get_kubeconfig(stack: &str) -> Result<Client> {
    // run pulumi config get config kubeconfig
    let kubeconfig_yaml =
        if let Ok(cached_kubeconfig) = get_cached_local::<String>(&get_kubeconfig_key(stack)) {
            debug!("Using cached kubeconfig");
            cached_kubeconfig.value
        } else {
            let cmd_output = run_cmd(vec!["pulumi", "config", "get", "kubeconfig"], None)?;
            let kubeconfig_yaml = String::from_utf8(cmd_output.stdout)?;
            cache_local(&get_kubeconfig_key(stack), kubeconfig_yaml.clone())?;

            kubeconfig_yaml
        };
    // create a new client
    let kubeconfig = Kubeconfig::from_yaml(&kubeconfig_yaml)?;
    let config = Config::from_custom_kubeconfig(kubeconfig, &Default::default())
        .await
        .context("Failed to create kubernetes client")?;
    let client = Client::try_from(config)?;
    Ok(client)
}

pub async fn get_logs(stack: &str, namespace: &str) -> Result<()> {
    // Create kubernetes client
    let client = get_kubeconfig(stack).await?;

    // Get deployments API in the specified namespace
    let pods: Api<Pod> = Api::namespaced(client, namespace);
    // Get list of pods
    let pod_list = pods
        .list(&Default::default())
        .await
        .context("Failed to get pods")?;

    // Extract pod names
    let pod_names: Vec<String> = pod_list
        .iter()
        .map(|pod| pod.metadata.name.clone().unwrap_or_default())
        .collect();

    if pod_names.is_empty() {
        println!("No pods found in namespace '{}'", namespace);
        return Ok(());
    }

    // Ask user to select a pod
    let pod_name = inquire::Select::new("Select pod to view logs from:", pod_names)
        .prompt()
        .map_err(|e| anyhow::anyhow!("Failed to get pod selection: {}", e))?;

    // Get logs from the deployment named "deploy"
    let logs = pods
        .logs(&pod_name, &LogParams::default())
        .await
        .context("Failed to get logs")?;

    println!("{}", logs);
    Ok(())
}
