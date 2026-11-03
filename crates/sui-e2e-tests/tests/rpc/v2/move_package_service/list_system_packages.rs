// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_macros::sim_test;
use sui_rpc::proto::sui::rpc::v2::{
    ListSystemPackagesRequest, PackageVersion,
    move_package_service_client::MovePackageServiceClient,
};
use sui_types::{
    MOVE_STDLIB_PACKAGE_ID, SUI_FRAMEWORK_PACKAGE_ID, SUI_SYSTEM_PACKAGE_ID,
    SYSTEM_PACKAGE_ADDRESSES, base_types::ObjectID,
};
use test_cluster::{TestCluster, TestClusterBuilder};

use crate::v2::assert_above_latest_available;
use crate::v2::assert_below_earliest_available;
use crate::v2::halt_chain;
use crate::v2::prune_objects_below_tip;

/// The fullnode's live system packages, ordered by package id. Once the chain is halted, this is
/// the expected answer at its tip. These clusters never upgrade their system packages, so each
/// one's live version is the one genesis published, at checkpoint 0.
async fn live_system_packages(cluster: &TestCluster) -> Vec<PackageVersion> {
    let mut package_ids: Vec<ObjectID> = SYSTEM_PACKAGE_ADDRESSES
        .iter()
        .copied()
        .map(ObjectID::from)
        .collect();
    package_ids.sort();

    let mut present = vec![];
    let mut packages = vec![];
    for package_id in package_ids {
        if let Some(object) = cluster.get_object_from_fullnode_store(&package_id).await {
            assert!(object.is_package(), "{package_id} is not a package");
            present.push(package_id);
            packages.push(
                PackageVersion::new(&package_id.into(), object.version().value())
                    .with_checkpoint(0)
                    .with_original_id(package_id.to_canonical_string(true)),
            );
        }
    }

    for package_id in [
        MOVE_STDLIB_PACKAGE_ID,
        SUI_FRAMEWORK_PACKAGE_ID,
        SUI_SYSTEM_PACKAGE_ID,
    ] {
        assert!(
            present.contains(&package_id),
            "genesis is missing {package_id}: {present:?}"
        );
    }
    packages
}

async fn list_system_packages(
    service: &mut MovePackageServiceClient<tonic::transport::Channel>,
    at_checkpoint: u64,
) -> Result<Vec<PackageVersion>, tonic::Status> {
    service
        .list_system_packages(ListSystemPackagesRequest::new(at_checkpoint))
        .await
        .map(|response| response.into_inner().system_packages)
}

/// At the tip, the answer is exactly the live system packages, ordered by package id; any
/// checkpoint above the tip fails the whole request.
#[sim_test]
async fn test_list_system_packages_at_tip() {
    let cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;
    let mut service = MovePackageServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let tip = halt_chain(&cluster).await;
    let expected = live_system_packages(&cluster).await;

    let packages = list_system_packages(&mut service, tip).await.unwrap();
    assert_eq!(packages, expected);

    for checkpoint in [tip + 1, u64::MAX] {
        let error = list_system_packages(&mut service, checkpoint)
            .await
            .unwrap_err();
        assert_above_latest_available(&error, "at_checkpoint", checkpoint);
    }
}

#[sim_test]
async fn test_list_system_packages_missing_at_checkpoint() {
    let cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;
    let mut service = MovePackageServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let error = service
        .list_system_packages(ListSystemPackagesRequest::default())
        .await
        .unwrap_err();
    assert_eq!(error.code(), tonic::Code::InvalidArgument);
    assert!(error.message().contains("missing at_checkpoint"));
}

/// Pruning object versions through a checkpoint leaves the system packages listable at that
/// checkpoint and rejects every checkpoint below it as pruned.
#[sim_test]
async fn test_list_system_packages_below_available_floor() {
    let cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        // Only the explicit prune below moves the floor.
        .disable_fullnode_pruning()
        .build()
        .await;
    let mut service = MovePackageServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    halt_chain(&cluster).await;
    let pruned_to = prune_objects_below_tip(&cluster).await;
    let expected = live_system_packages(&cluster).await;

    for checkpoint in [0, pruned_to - 1] {
        let error = list_system_packages(&mut service, checkpoint)
            .await
            .unwrap_err();
        assert_below_earliest_available(&error, pruned_to);
    }
    let packages = list_system_packages(&mut service, pruned_to).await.unwrap();
    assert_eq!(packages, expected);
}
