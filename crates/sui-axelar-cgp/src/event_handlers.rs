use sui_sdk::SuiClient;

use crate::listeners::evm_listener::ContractCall;

pub fn handle_evm_contract_call(contract_call: ContractCall, sui_client: &SuiClient) {}
