// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{anyhow, bail};
use async_trait::async_trait;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use move_core_types::language_storage::StructTag;
use serde_json::Value;
use sui_transaction_builder::DataReader;
use sui_types::base_types::{ObjectID, ObjectInfo, SuiAddress};
use sui_types::object::Object;

/// JSON-RPC client for a running `sui fork start` server.
/// Implements `DataReader` so `TransactionBuilder` can build Move call transactions.
pub struct ForkClient {
    pub rpc_url: String,
    http: reqwest::Client,
}

impl ForkClient {
    pub fn new(rpc_url: String) -> Self {
        Self {
            rpc_url,
            http: reqwest::Client::new(),
        }
    }

    pub async fn call(&self, method: &str, params: Value) -> anyhow::Result<Value> {
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
            "id": 1,
        });
        let resp: Value = self
            .http
            .post(&self.rpc_url)
            .json(&body)
            .send()
            .await?
            .json()
            .await?;

        if let Some(err) = resp.get("error") {
            bail!("RPC error from {method}: {err}");
        }
        resp.get("result")
            .cloned()
            .ok_or_else(|| anyhow!("no result field in RPC response from {method}"))
    }

    /// Call `fork_fundAccount` and return the created coin ID.
    pub async fn fund_account(&self, address: SuiAddress, amount: u64) -> anyhow::Result<ObjectID> {
        let result = self
            .call(
                "fork_fundAccount",
                serde_json::json!([address.to_string(), amount]),
            )
            .await?;
        let id_str = result
            .as_str()
            .ok_or_else(|| anyhow!("expected string from fork_fundAccount"))?;
        id_str.parse().map_err(|e| anyhow!("invalid ObjectID: {e}"))
    }

    pub async fn execute_transaction(
        &self,
        tx_bytes_b64: String,
    ) -> anyhow::Result<Value> {
        self.call(
            "sui_executeTransactionBlock",
            serde_json::json!([tx_bytes_b64]),
        )
        .await
    }

    pub async fn dry_run_transaction(
        &self,
        tx_bytes_b64: String,
    ) -> anyhow::Result<Value> {
        self.call(
            "sui_dryRunTransactionBlock",
            serde_json::json!([tx_bytes_b64]),
        )
        .await
    }

    pub async fn get_object_json(&self, id: ObjectID) -> anyhow::Result<Value> {
        self.call("sui_getObject", serde_json::json!([id.to_string()]))
            .await
    }

    pub async fn get_transaction_block(&self, digest: &str) -> anyhow::Result<Value> {
        self.call(
            "sui_getTransactionBlock",
            serde_json::json!([digest]),
        )
        .await
    }

    pub async fn query_events_by_tx(&self, digest: &str) -> anyhow::Result<Value> {
        self.call(
            "suix_queryEvents",
            serde_json::json!([{ "Transaction": digest }]),
        )
        .await
    }

    pub async fn query_events_by_type(&self, event_type: &str) -> anyhow::Result<Value> {
        self.call(
            "suix_queryEvents",
            serde_json::json!([{ "MoveEventType": event_type }]),
        )
        .await
    }

    pub async fn get_balance(
        &self,
        address: SuiAddress,
        coin_type: Option<&str>,
    ) -> anyhow::Result<Value> {
        let params = match coin_type {
            Some(ct) => serde_json::json!([address.to_string(), ct]),
            None => serde_json::json!([address.to_string()]),
        };
        self.call("suix_getBalance", params).await
    }

    pub async fn get_all_balances(&self, address: SuiAddress) -> anyhow::Result<Value> {
        self.call(
            "suix_getAllBalances",
            serde_json::json!([address.to_string()]),
        )
        .await
    }

    pub async fn get_coins(
        &self,
        address: SuiAddress,
        coin_type: Option<&str>,
    ) -> anyhow::Result<Value> {
        let params = match coin_type {
            Some(ct) => serde_json::json!([address.to_string(), ct]),
            None => serde_json::json!([address.to_string()]),
        };
        self.call("suix_getCoins", params).await
    }

    pub async fn fork_snapshot(&self) -> anyhow::Result<u64> {
        let result = self.call("fork_snapshot", serde_json::json!([])).await?;
        result
            .as_u64()
            .ok_or_else(|| anyhow!("expected number from fork_snapshot"))
    }

    pub async fn fork_revert(&self, snapshot_id: u64) -> anyhow::Result<()> {
        self.call("fork_revert", serde_json::json!([snapshot_id]))
            .await?;
        Ok(())
    }

    pub async fn fork_reset(&self, checkpoint: Option<u64>) -> anyhow::Result<()> {
        let params = match checkpoint {
            Some(cp) => serde_json::json!([cp]),
            None => serde_json::json!([]),
        };
        self.call("fork_reset", params).await?;
        Ok(())
    }

    pub async fn fork_advance_clock(&self, duration_ms: u64) -> anyhow::Result<()> {
        self.call("fork_advanceClock", serde_json::json!([duration_ms]))
            .await?;
        Ok(())
    }

    pub async fn fork_set_clock_timestamp(&self, timestamp_ms: u64) -> anyhow::Result<()> {
        self.call("fork_setClockTimestamp", serde_json::json!([timestamp_ms]))
            .await?;
        Ok(())
    }

    pub async fn fork_advance_epoch(&self) -> anyhow::Result<()> {
        self.call("fork_advanceEpoch", serde_json::json!([])).await?;
        Ok(())
    }

    /// Returns `true` if the object was found at the fork checkpoint and seeded locally,
    /// `false` if it does not exist at that checkpoint.
    pub async fn fork_seed_object(&self, object_id: ObjectID) -> anyhow::Result<bool> {
        let val = self
            .call("fork_seedObject", serde_json::json!([object_id.to_string()]))
            .await?;
        Ok(val.as_bool().unwrap_or(false))
    }

    pub async fn fork_dump_state(&self, path: &str) -> anyhow::Result<()> {
        self.call("fork_dumpState", serde_json::json!([path])).await?;
        Ok(())
    }

    pub async fn fork_load_state(&self, path: &str) -> anyhow::Result<()> {
        self.call("fork_loadState", serde_json::json!([path])).await?;
        Ok(())
    }

    pub async fn fork_set_object_bcs(
        &self,
        object_id: ObjectID,
        bcs_b64: String,
    ) -> anyhow::Result<()> {
        self.call(
            "fork_setObjectBcs",
            serde_json::json!([object_id.to_string(), bcs_b64]),
        )
        .await?;
        Ok(())
    }

    pub async fn fork_set_owner(
        &self,
        object_id: ObjectID,
        owner_json: Value,
    ) -> anyhow::Result<()> {
        self.call(
            "fork_setOwner",
            serde_json::json!([object_id.to_string(), owner_json]),
        )
        .await?;
        Ok(())
    }

    pub async fn fork_get_object_history(&self, object_id: ObjectID) -> anyhow::Result<Value> {
        self.call(
            "fork_getObjectHistory",
            serde_json::json!([object_id.to_string()]),
        )
        .await
    }

    pub async fn fork_list_transactions(&self) -> anyhow::Result<Value> {
        self.call("fork_listTransactions", serde_json::json!([])).await
    }

    pub async fn fork_get_dynamic_fields(&self, parent_id: ObjectID) -> anyhow::Result<Value> {
        self.call(
            "suix_getDynamicFields",
            serde_json::json!([parent_id.to_string()]),
        )
        .await
    }

    pub async fn fork_decode_object(&self, object_id: ObjectID) -> anyhow::Result<Value> {
        self.call(
            "fork_decodeObject",
            serde_json::json!([object_id.to_string()]),
        )
        .await
    }

    pub async fn fork_replay_transaction(&self, digest: &str) -> anyhow::Result<Value> {
        self.call(
            "fork_replayTransaction",
            serde_json::json!([digest]),
        )
        .await
    }

    pub async fn fork_seed_bridge_objects(&self) -> anyhow::Result<()> {
        self.call("fork_seedBridgeObjects", serde_json::json!([])).await?;
        Ok(())
    }

    pub async fn fork_setup_bridge_test_committee(&self) -> anyhow::Result<()> {
        self.call("fork_setupBridgeTestCommittee", serde_json::json!([])).await?;
        Ok(())
    }

    pub async fn fork_simulate_eth_to_sui_bridge(
        &self,
        recipient: SuiAddress,
        token_id: u8,
        amount: u64,
        nonce: u64,
        eth_chain_id: u8,
    ) -> anyhow::Result<Value> {
        self.call(
            "fork_simulateEthToSuiBridge",
            serde_json::json!([
                recipient.to_string(),
                token_id,
                amount,
                nonce,
                eth_chain_id
            ]),
        )
        .await
    }

    pub async fn fork_simulate_sui_to_eth_bridge(
        &self,
        sender: SuiAddress,
        token_object_id: ObjectID,
        eth_chain_id: u8,
        eth_recipient: &str,
        gas_budget: u64,
    ) -> anyhow::Result<Value> {
        self.call(
            "fork_simulateSuiToEthBridge",
            serde_json::json!([
                sender.to_string(),
                token_object_id.to_string(),
                eth_chain_id,
                eth_recipient,
                gas_budget
            ]),
        )
        .await
    }

    fn decode_object_bcs(result: &Value) -> anyhow::Result<Object> {
        if let Some(err) = result.get("error") {
            bail!("object not found: {err}");
        }
        let bcs_b64 = result
            .get("bcs")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("missing bcs field in fork_getObjectBcs response"))?;
        let bytes = BASE64_STANDARD
            .decode(bcs_b64)
            .map_err(|e| anyhow!("base64 decode: {e}"))?;
        bcs::from_bytes(&bytes).map_err(|e| anyhow!("bcs decode Object: {e}"))
    }
}

#[async_trait]
impl DataReader for ForkClient {
    async fn get_owned_objects(
        &self,
        address: SuiAddress,
        object_type: StructTag,
    ) -> anyhow::Result<Vec<ObjectInfo>> {
        let result = self
            .call(
                "fork_getOwnedObjectsBcs",
                serde_json::json!([address.to_string(), object_type.to_string()]),
            )
            .await?;

        let arr = result
            .as_array()
            .ok_or_else(|| anyhow!("expected array from fork_getOwnedObjectsBcs"))?;

        let mut infos = Vec::new();
        for entry in arr {
            let bcs_b64 = entry
                .get("bcs")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("missing bcs in owned object entry"))?;
            let bytes = BASE64_STANDARD
                .decode(bcs_b64)
                .map_err(|e| anyhow!("base64 decode: {e}"))?;
            let obj: Object =
                bcs::from_bytes(&bytes).map_err(|e| anyhow!("bcs decode Object: {e}"))?;
            infos.push(ObjectInfo::from_object(&obj));
        }
        Ok(infos)
    }

    async fn get_object(&self, object_id: ObjectID) -> anyhow::Result<Object> {
        let result = self
            .call(
                "fork_getObjectBcs",
                serde_json::json!([object_id.to_string()]),
            )
            .await?;
        Self::decode_object_bcs(&result)
    }

    async fn get_reference_gas_price(&self) -> anyhow::Result<u64> {
        let result = self
            .call("sui_getReferenceGasPrice", serde_json::json!([]))
            .await?;
        let price_str = result
            .as_str()
            .ok_or_else(|| anyhow!("expected string from sui_getReferenceGasPrice"))?;
        price_str
            .parse()
            .map_err(|e| anyhow!("parse gas price: {e}"))
    }
}
