use fastcrypto::encoding::Base64;
use mamoru_core::errors::SnifferError;
use mamoru_core::value::Value;
use mamoru_core::vendor::ethnum::U256;
pub use mamoru_core::*;
use move_core_types::trace::CallType;
use std::collections::HashMap;
use sui_types::batch::TxSequenceNumber;
use sui_types::messages::{SignedTransactionEffects, VerifiedCertificate};

pub const RULES_UPDATE_INTERVAL_SECS: u64 = 120;

pub struct SuiSniffer {
    inner: Sniffer,
    rules_updated_at: u64,
}

impl SuiSniffer {
    pub async fn new() -> Result<Self, SnifferError> {
        let mut inner = Sniffer::new(SnifferConfig::from_env()).await?;
        inner.register().await?;

        Ok(Self {
            inner,
            rules_updated_at: 0,
        })
    }

    pub async fn unregister(&self) -> Result<(), SnifferError> {
        self.inner.unregister().await
    }

    pub async fn update_rules(&mut self, now: u64) -> Result<(), SnifferError> {
        self.inner.update_rules().await?;
        self.rules_updated_at = now;

        Ok(())
    }

    pub fn should_update_rules(&self, now: u64) -> bool {
        let interval = now - self.rules_updated_at;

        interval > RULES_UPDATE_INTERVAL_SECS
    }

    pub async fn observe_transaction(
        &self,
        certificate: &VerifiedCertificate,
        signed_effects: &SignedTransactionEffects,
        seq: TxSequenceNumber,
        time: u64,
    ) -> Result<(), SnifferError> {
        let effects = &signed_effects.effects;
        let tx_data = &certificate.data().data;
        // TODO: replace with digest as u256
        let tx_index = seq as u128;
        let tx_hash = Base64::from_bytes(effects.transaction_digest.as_ref()).encoded();
        let events = effects
            .events
            .iter()
            .cloned()
            .enumerate()
            .map(|(idx, event)| make_event(idx as u128, tx_index, event))
            .collect();

        let call_traces = effects
            .call_traces
            .iter()
            .cloned()
            .enumerate()
            .map(|(idx, trace)| make_call_trace(idx as u128, tx_index, trace))
            .collect();

        let mut extra = HashMap::new();

        extra.insert(
            "sender".to_string(),
            Value::Binary(certificate.sender_address().to_vec()),
        );
        extra.insert("sequence".to_string(), Value::UInt64(U256::from(seq)));
        extra.insert(
            "gas_limit".to_string(),
            Value::UInt64(U256::from(tx_data.gas_budget)),
        );
        extra.insert(
            "gas_used".to_string(),
            Value::UInt64(U256::from(tx_data.gas_price)),
        );

        let tx = blockchain_data_types::Transaction::new(
            tx_index,
            tx_index,
            time,
            events,
            call_traces,
            extra,
        );

        self.inner.observe_transaction(tx, tx_hash).await?;

        Ok(())
    }
}

fn make_event(
    idx: u128,
    tx_index: u128,
    sui_event: sui_types::event::Event,
) -> blockchain_data_types::Event {
    let event_id = vec![];

    let mut extra = HashMap::new();
    extra.insert(
        "type".to_string(),
        Value::Binary(sui_event.event_type().to_string().as_bytes().to_vec()),
    );

    blockchain_data_types::Event::new(tx_index, tx_index, idx, event_id, extra)
}

fn make_call_trace(
    idx: u128,
    tx_index: u128,
    move_call_trace: move_core_types::trace::CallTrace,
) -> blockchain_data_types::CallTrace {
    // there is no events information in the call trace
    let events = vec![];

    let call_type: u8 = match move_call_trace.call_type {
        CallType::Call => 0,
        CallType::CallGeneric => 1,
    };

    let mut extra = HashMap::new();

    extra.insert("type".to_string(), Value::UInt8(U256::from(call_type)));
    extra.insert(
        "depth".to_string(),
        Value::UInt32(U256::from(move_call_trace.depth)),
    );
    extra.insert(
        "gas_used".to_string(),
        Value::UInt64(U256::from(move_call_trace.gas_used)),
    );
    extra.insert(
        "module_address".to_string(),
        Value::Binary(
            move_call_trace
                .module_id
                .map(|id| id.as_bytes().to_vec())
                .unwrap_or_default(),
        ),
    );
    extra.insert(
        "method".to_string(),
        Value::Binary(move_call_trace.function.as_bytes().to_vec()),
    );
    extra.insert(
        "type_arguments".to_string(),
        Value::Array(
            move_call_trace
                .ty_args
                .into_iter()
                .map(Value::Binary)
                .collect(),
        ),
    );
    extra.insert(
        "arguments".to_string(),
        Value::Array(
            move_call_trace
                .args_values
                .into_iter()
                .map(Value::Binary)
                .collect(),
        ),
    );

    blockchain_data_types::CallTrace::new(tx_index, tx_index, idx, events, extra)
}
