// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod compatibility_tests {
    use std::collections::BTreeMap;
    use sui_framework::{compare_system_package, BuiltInFramework};
    use sui_framework_snapshot::{load_bytecode_snapshot, load_bytecode_snapshot_manifest};
    use sui_protocol_config::{Chain, ProtocolConfig, ProtocolVersion};
    use sui_types::execution_config_utils::to_binary_config;

    #[tokio::test]
    async fn test_framework_compatibility() {
        // This test checks that the current framework is compatible with all previous framework
        // bytecode snapshots.
        for (version, _snapshots) in load_bytecode_snapshot_manifest() {
            let config =
                ProtocolConfig::get_for_version(ProtocolVersion::new(version), Chain::Unknown);
            let binary_config = to_binary_config(&config);
            let framework = load_bytecode_snapshot(version).unwrap();
            let old_framework_store: BTreeMap<_, _> = framework
                .into_iter()
                .map(|package| (*package.id(), package.genesis_object()))
                .collect();
            for cur_package in BuiltInFramework::iter_system_packages() {
                if compare_system_package(
                    &old_framework_store,
                    cur_package.id(),
                    &cur_package.modules(),
                    cur_package.dependencies().to_vec(),
                    &binary_config,
                )
                .await
                .is_none()
                {
                    panic!(
                        "The current Sui framework {:?} is not compatible with version {:?}",
                        cur_package.id(),
                        version
                    );
                }
            }
        }
    }

    #[test]
    fn check_framework_change_with_protocol_upgrade() {
        // This test checks that if we ever update the framework, the current protocol version must differ
        // the latest bytecode snapshot in each network.
        let snapshots = load_bytecode_snapshot_manifest();
        let latest_snapshot_version = *snapshots.keys().max().unwrap();
        if latest_snapshot_version != ProtocolVersion::MAX.as_u64() {
            // If we have already incremented the protocol version, then we are fine and we don't
            // care if the framework has changed.
            return;
        }
        let latest_snapshot = load_bytecode_snapshot(*snapshots.keys().max().unwrap()).unwrap();
        // Turn them into BTreeMap for deterministic comparison.
        let latest_snapshot_ref: BTreeMap<_, _> =
            latest_snapshot.iter().map(|p| (p.id(), p)).collect();
        let current_framework: BTreeMap<_, _> = BuiltInFramework::iter_system_packages()
            .map(|p| (p.id(), p))
            .collect();
        assert_eq!(
                latest_snapshot_ref,
                current_framework,
                "The current framework differs the latest bytecode snapshot. Did you forget to upgrade protocol version?"
            );
    }

    #[test]
    fn check_no_dirty_manifest_commit() {
        let snapshots = load_bytecode_snapshot_manifest();
        for snapshot in snapshots.values() {
            assert!(
                !snapshot.git_revision().contains("dirty"),
                "If you are trying to regenerate the bytecode snapshot after cherry-picking, please do so in a standalone PR after the cherry-pick is merged on the release branch.",
            );
        }
    }
}
