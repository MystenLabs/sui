// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{TestCaseImpl, TestContext};
use async_trait::async_trait;
use jsonrpsee::rpc_params;
use sui_core::test_utils::compile_basics_package;
use sui_json_rpc_types::SuiTransactionEffectsAPI;
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
        let all_module_bytes =
            compile_basics_package().get_package_base64(/* with_unpublished_deps */ false);

        let params = rpc_params![
            ctx.get_wallet_address(),
            all_module_bytes,
            None::<ObjectID>,
            10000
        ];

        let data = ctx
            .build_transaction_remotely("sui_publish", params)
            .await?;
        let response = ctx.sign_and_execute(data, "publish basics package").await;
        response
            .effects
            .created()
            .iter()
            .find(|obj_ref| obj_ref.owner == Owner::Immutable)
            .unwrap();

        Ok(())
    }
}
