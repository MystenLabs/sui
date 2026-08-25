// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{TestCaseImpl, TestContext};
use async_trait::async_trait;
use sui_move_build::test_utils::compile_basics_package;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::object::Owner;

pub struct GrpcPublishTransactionTest;

#[async_trait]
impl TestCaseImpl for GrpcPublishTransactionTest {
    fn name(&self) -> &'static str {
        "GrpcPublishTransaction"
    }

    fn description(&self) -> &'static str {
        "Test building and executing a publish transaction via the gRPC transaction builder"
    }

    async fn run(&self, ctx: &mut TestContext) -> Result<(), anyhow::Error> {
        let signer = ctx.get_wallet_address();
        // Fund a gas coin and supply it explicitly so the gRPC publish builder
        // stays on `LedgerService`.
        let gas = ctx.get_sui_from_faucet(Some(1)).await.swap_remove(0);
        let gas_ref = ctx.current_object_ref(*gas.id()).await;

        // Compile the package and pass its module bytes + dependencies to the
        // gRPC-backed publish builder.
        let compiled_package = compile_basics_package().await;
        let compiled_modules =
            compiled_package.get_package_bytes(/* with_unpublished_deps */ false);
        let dependencies = compiled_package.get_dependency_storage_package_ids();

        let builder = ctx.get_grpc_client().transaction_builder();
        let data = builder
            .publish(
                signer,
                compiled_modules,
                dependencies,
                Some(gas_ref.0),
                // Doesn't need to be scaled by RGP since most of the cost is storage
                500_000_000,
            )
            .await?;

        let response = ctx.sign_and_execute(data, "publish basics package").await;
        // Publishing must create an immutable package object.
        response
            .effects
            .created()
            .iter()
            .find(|(_obj_ref, owner)| *owner == Owner::Immutable)
            .expect("Publish should create an immutable package object");

        Ok(())
    }
}
