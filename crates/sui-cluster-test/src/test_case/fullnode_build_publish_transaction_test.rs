// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{TestCaseImpl, TestContext};
use async_trait::async_trait;
use jsonrpsee::rpc_params;
use sui_json_rpc_types::SuiTransactionBlockEffectsAPI;
use sui_move_build::test_utils::compile_basics_package;
use sui_types::{base_types::ObjectID, object::Owner};

pub struct FullNodeBuildPublishTransactionTest;

#[async_trait]
impl TestCaseImpl for FullNodeBuildPublishTransactionTest {
    fn name(&self) -> &'static str {
        "FullNodeBuildPublishTransaction"
    }

    fn description(&self) -> &'static str {
        "Test building publish transaction via full node"
    }

    async fn run(&self, ctx: &mut TestContext) -> Result<(), anyhow::Error> {
        let compiled_package = compile_basics_package();
        let all_module_bytes =
            compiled_package.get_package_base64(/* with_unpublished_deps */ false);
        let dependencies = compiled_package.get_dependency_storage_package_ids();

        let params = rpc_params![
            ctx.get_wallet_address(),
            all_module_bytes,
            dependencies,
            None::<ObjectID>,
            // Doesn't need to be scaled by RGP since most of the cost is storage
            50_000_000.to_string()
        ];

        let data = ctx
            .build_transaction_remotely("unsafe_publish", params)
            .await?;
        let response = ctx.sign_and_execute(data, "publish basics package").await;
        response
            .effects
            .as_ref()
            .unwrap()
            .created()
            .iter()
            .find(|obj_ref| obj_ref.owner == Owner::Immutable)
            .unwrap();

        Ok(())
    }
}
