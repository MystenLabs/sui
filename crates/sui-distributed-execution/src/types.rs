use sui_protocol_config::ProtocolConfig;
use sui_types::base_types::ExecutionDigests;
use sui_types::epoch_data::EpochData;
use sui_types::messages::VerifiedTransaction;
use sui_types::sui_system_state::epoch_start_sui_system_state::EpochStartSystemState;

#[derive(Debug)]
pub struct EpochStartMessage(pub ProtocolConfig, pub EpochData, pub u64);
#[derive(Debug)]
pub struct EpochEndMessage(pub EpochStartSystemState);
#[derive(Debug)]
pub struct TransactionMessage(pub VerifiedTransaction, pub ExecutionDigests, pub u64);