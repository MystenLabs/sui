// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::time::Duration;

use axum::{Json, extract::State, response::IntoResponse};
use rand::rngs::OsRng;
use serde::Deserialize;
use tracing::{info, warn};

use simulacrum::Simulacrum;
use sui_types::{
    base_types::SuiAddress,
    effects::TransactionEffects,
    gas_coin::MIST_PER_SUI,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    transaction::{GasData, TransactionData, TransactionKind},
};

use crate::api::types::{AdvanceClockRequest, ApiResponse, ExecuteTxResponse, ForkingStatus};
use crate::store::ForkingStore;

/// The shared state for the forking server
pub(super) struct AppState {
    pub context: crate::context::Context,
}

impl AppState {
    pub async fn new(context: crate::context::Context) -> Self {
        Self { context }
    }
}

pub(super) async fn health() -> &'static str {
    "OK"
}

pub(super) async fn get_status(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let sim = state.context.simulacrum.read().await;
    let store = sim.store();

    let checkpoint = store
        .get_highest_checkpint()
        .map(|c| c.sequence_number)
        .unwrap_or(0);

    // Get the current epoch from the checkpoint
    let epoch = store
        .get_highest_checkpint()
        .map(|c| c.epoch())
        .unwrap_or(0);

    let clock_timestamp_ms = store.get_clock().timestamp_ms();

    let status = ForkingStatus {
        checkpoint,
        epoch,
        clock_timestamp_ms,
    };

    Json(ApiResponse {
        success: true,
        data: Some(status),
        error: None,
    })
}

pub(super) async fn advance_checkpoint(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let checkpoint_sequence_number = {
        let mut sim = state.context.simulacrum.write().await;

        // create_checkpoint returns a VerifiedCheckpoint, not a Result
        let checkpoint = sim.create_checkpoint();
        info!("Advanced to checkpoint {}", checkpoint.sequence_number);
        checkpoint.sequence_number
    };

    if let Err(err) = state
        .context
        .publish_checkpoint_by_sequence_number(checkpoint_sequence_number)
        .await
    {
        warn!(
            checkpoint_sequence_number,
            "Failed to publish checkpoint to subscribers: {err}"
        );
    }

    Json(ApiResponse::<String> {
        success: true,
        data: Some(format!(
            "Advanced to checkpoint {}",
            checkpoint_sequence_number
        )),
        error: None,
    })
}

pub(super) async fn advance_clock(
    State(state): State<Arc<AppState>>,
    Json(request): Json<AdvanceClockRequest>,
) -> impl IntoResponse {
    let mut sim = state.context.simulacrum.write().await;

    let duration = Duration::from_millis(request.ms);
    sim.advance_clock(duration);
    info!("Advanced clock by {} ms", request.ms);

    Json(ApiResponse::<String> {
        success: true,
        data: Some(format!("Clock advanced by {} ms", request.ms)),
        error: None,
    })
}

pub(super) async fn advance_epoch(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let latest_checkpoint_sequence_number = {
        let mut sim = state.context.simulacrum.write().await;

        // Use default configuration for advancing epoch
        let config = simulacrum::AdvanceEpochConfig::default();
        sim.advance_epoch(config);
        info!("Advanced to next epoch");
        sim.store()
            .get_highest_checkpint()
            .map(|cp| cp.sequence_number)
    };

    if let Some(checkpoint_sequence_number) = latest_checkpoint_sequence_number
        && let Err(err) = state
            .context
            .publish_checkpoint_by_sequence_number(checkpoint_sequence_number)
            .await
    {
        warn!(
            checkpoint_sequence_number,
            "Failed to publish checkpoint to subscribers after epoch advance: {err}"
        );
    }

    Json(ApiResponse::<String> {
        success: true,
        data: Some("Advanced to next epoch".to_string()),
        error: None,
    })
}

#[derive(Deserialize)]
pub(super) struct FaucetRequest {
    address: SuiAddress,
    amount: u64,
}

pub(super) async fn faucet(
    State(state): State<Arc<AppState>>,
    Json(request): Json<FaucetRequest>,
) -> impl IntoResponse {
    let FaucetRequest { address, amount } = request;
    let Some(faucet_owner) = state.context.faucet_owner else {
        return Json(ApiResponse::<ExecuteTxResponse> {
            success: false,
            data: None,
            error: Some(
                "Faucet is unavailable: no local faucet owner was configured at startup"
                    .to_string(),
            ),
        });
    };

    let mut simulacrum = state.context.simulacrum.write().await;
    let response = execute_faucet_transfer(&mut simulacrum, faucet_owner, address, amount);

    match response {
        Ok(effects) => {
            let effects_bytes = bcs::to_bytes(&effects).unwrap();
            let effects_base64 =
                base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &effects_bytes);

            info!("Executed transaction successfully");

            Json(ApiResponse {
                success: true,
                data: Some(ExecuteTxResponse {
                    effects: effects_base64,
                    error: None,
                }),
                error: None,
            })
        }
        Err(err) => Json(ApiResponse::<ExecuteTxResponse> {
            success: false,
            data: None,
            error: Some(format!("Failed to execute faucet transfer: {}", err)),
        }),
    }
}

fn execute_faucet_transfer(
    simulacrum: &mut Simulacrum<OsRng, ForkingStore>,
    faucet_owner: SuiAddress,
    recipient: SuiAddress,
    amount: u64,
) -> Result<TransactionEffects, anyhow::Error> {
    let required_balance = amount.saturating_add(MIST_PER_SUI);
    let Some(faucet_coin) = simulacrum
        .store()
        .owned_objects(faucet_owner)
        .filter(|object| object.is_gas_coin() && object.get_coin_value_unsafe() >= required_balance)
        .max_by_key(|object| object.get_coin_value_unsafe())
    else {
        anyhow::bail!(
            "No faucet coin with enough balance for {} Mist (required balance >= {})",
            amount,
            required_balance
        );
    };

    let programmable_tx = {
        let mut builder = ProgrammableTransactionBuilder::new();
        builder.transfer_sui(recipient, Some(amount));
        builder.finish()
    };
    let kind = TransactionKind::ProgrammableTransaction(programmable_tx);
    let gas_data = GasData {
        payment: vec![faucet_coin.compute_object_reference()],
        owner: faucet_owner,
        price: simulacrum.reference_gas_price(),
        budget: MIST_PER_SUI,
    };
    let tx_data = TransactionData::new_with_gas_data(kind, faucet_owner, gas_data);
    let (effects, execution_error) = simulacrum.execute_transaction_impersonating(tx_data)?;

    if let Some(err) = execution_error {
        anyhow::bail!("faucet transfer execution error: {err:?}");
    }

    Ok(effects)
}
