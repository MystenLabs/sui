// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use ethabi::Contract;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::abi_signature_type::ABIJsonSignature;

/// Key for metadata
pub const ABI_ETHER_MOVE_KEY: &str = "abi_ethereum_move";

#[derive(Serialize, Deserialize)]
pub struct ABIMoveSignature {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub constructor: Option<ABIJsonSignature>,
    // Move type -> Ethereum event abi
    pub event_map: BTreeMap<String, ABIJsonSignature>,
    // Move function -> Ethereum pub function abi
    pub func_map: BTreeMap<String, ABIJsonSignature>,
    // Indicate whether a receive function is defined in the contract
    pub receive: bool,
    // Indicate whether a fallback function is defined in the contract
    pub fallback: bool,
}

impl ABIMoveSignature {
    fn _convert_to_contract(&self) -> Result<Contract, ethabi::Error> {
        let contract_str = self._generate_abi_string();
        Contract::load(contract_str.as_bytes())
    }

    fn _generate_abi_string(&self) -> String {
        let mut res = vec![];
        if let Some(cons) = &self.constructor {
            res.push(serde_json::to_string_pretty(&cons).unwrap());
        }
        for (_, ev_sig) in self.event_map.iter() {
            res.push(serde_json::to_string_pretty(&ev_sig).unwrap());
        }
        for (_, fn_sig) in self.func_map.iter() {
            res.push(serde_json::to_string_pretty(&fn_sig).unwrap());
        }
        if self.receive {
            res.push("{ \"type\": \"receive\" }\n".to_string());
        }
        if self.fallback {
            res.push("{ \"type\": \"fallback\" }\n".to_string());
        }
        format!(
            "[\n{}\n]",
            res.iter()
                .map(|t| t.to_string())
                .collect::<Vec<_>>()
                .join(",\n")
        )
    }
}

#[cfg(test)]
#[allow(deprecated)]
mod tests {
    use super::*;
    use ethabi::{
        Constructor, Contract, Event, EventParam, Function, Param, ParamType, StateMutability,
    };
    use std::iter::FromIterator;

    #[test]
    fn test_fallback_receive_only() {
        let json = r#"{
            "event_map": {},
            "func_map": {},
            "receive": true,
            "fallback": true
          }"#;
        let deserialized_json_sig: ABIMoveSignature = serde_json::from_str(json).unwrap();
        let contract = deserialized_json_sig._convert_to_contract().unwrap();
        assert_eq!(
            contract,
            Contract {
                constructor: None,
                functions: BTreeMap::new(),
                events: BTreeMap::new(),
                errors: BTreeMap::new(),
                receive: true,
                fallback: true,
            }
        );
    }

    #[test]
    fn test_constructor() {
        let json = r#"{
            "constructor": {
              "name": "init",
              "type": "constructor",
              "inputs": [
                {
                  "type": "uint64",
                  "name": "value"
                },
                {
                  "type": "uint64",
                  "name": "value2"
                }
              ],
              "outputs": [],
              "stateMutability": "nonpayable"
            },
            "event_map": {},
            "func_map": {},
            "receive": false,
            "fallback": false
          }"#;
        let mut const_inputs = vec![];
        let param_uint64 = ParamType::Uint(64);
        let name_1 = "value";
        let name_2 = "value2";
        let para1 = Param {
            name: name_1.to_string(),
            kind: param_uint64.clone(),
            internal_type: None,
        };
        let para2 = Param {
            name: name_2.to_string(),
            kind: param_uint64,
            internal_type: None,
        };
        const_inputs.push(para1);
        const_inputs.push(para2);
        let constructor = Constructor {
            inputs: const_inputs,
        };
        let deserialized_json_sig: ABIMoveSignature = serde_json::from_str(json).unwrap();
        let contract = deserialized_json_sig._convert_to_contract().unwrap();
        assert_eq!(
            contract,
            Contract {
                constructor: Some(constructor),
                functions: BTreeMap::new(),
                events: BTreeMap::new(),
                errors: BTreeMap::new(),
                receive: false,
                fallback: false,
            }
        );
    }

    #[test]
    fn test_events() {
        let json = r#"{
            "event_map": {
                "ev1": {
					"type": "event",
					"name": "ev1",
					"inputs": [
						{
							"name":"a",
							"type":"bytes",
                            "indexed": true
						}
					],
					"anonymous": false
				},
				"ev2": {
					"type": "event",
					"name": "ev2",
					"inputs": [
						{
							"name":"b",
							"type":"address[]"
						}
					],
					"anonymous": false
				}
            },
            "func_map": {},
            "receive": true,
            "fallback": false
          }"#;
        let deserialized_json_sig: ABIMoveSignature = serde_json::from_str(json).unwrap();
        let contract = deserialized_json_sig._convert_to_contract().unwrap();
        assert_eq!(
            contract,
            Contract {
                constructor: None,
                functions: BTreeMap::new(),
                events: BTreeMap::from_iter(vec![
                    (
                        "ev1".to_string(),
                        vec![Event {
                            name: "ev1".to_string(),
                            inputs: vec![EventParam {
                                name: "a".to_string(),
                                kind: ParamType::Bytes,
                                indexed: true,
                            }],
                            anonymous: false,
                        }]
                    ),
                    (
                        "ev2".to_string(),
                        vec![Event {
                            name: "ev2".to_string(),
                            inputs: vec![EventParam {
                                name: "b".to_string(),
                                kind: ParamType::Array(Box::new(ParamType::Address)),
                                indexed: false
                            }],
                            anonymous: false,
                        }]
                    ),
                ]),
                errors: BTreeMap::new(),
                receive: true,
                fallback: false,
            }
        );
    }

    #[test]
    fn test_functions() {
        let json = r#"{
            "event_map": {},
            "func_map": {
                "fn1": {
                "type": "function",
                "name": "fn1",
                "inputs": [
                    {
                        "name":"arg1",
                        "type":"address[5]"
                    }
                ],
                "outputs": [
                    {
                        "name": "ret1",
                        "type":"address"
                    }
                ],
                "stateMutability": "nonpayable"
                },
                "fn2": {
                    "type": "function",
                    "name": "fn2",
                    "inputs": [],
                    "outputs": [],
                    "stateMutability": "pure"
                }
            },
            "receive": true,
            "fallback": false
          }"#;

        let deserialized_json_sig: ABIMoveSignature = serde_json::from_str(json).unwrap();
        let contract = deserialized_json_sig._convert_to_contract().unwrap();
        assert_eq!(
            contract,
            Contract {
                constructor: None,
                functions: BTreeMap::from_iter(vec![
                    (
                        "fn1".to_string(),
                        vec![Function {
                            name: "fn1".to_string(),
                            inputs: vec![Param {
                                name: "arg1".to_string(),
                                kind: ParamType::FixedArray(Box::new(ParamType::Address), 5),
                                internal_type: None,
                            }],
                            outputs: vec![Param {
                                name: "ret1".to_string(),
                                kind: ParamType::Address,
                                internal_type: None,
                            }],
                            constant: None,
                            state_mutability: Default::default(),
                        }]
                    ),
                    (
                        "fn2".to_string(),
                        vec![Function {
                            name: "fn2".to_string(),
                            inputs: vec![],
                            outputs: vec![],
                            constant: None,
                            state_mutability: StateMutability::Pure,
                        }]
                    ),
                ]),
                events: BTreeMap::new(),
                errors: BTreeMap::new(),
                receive: true,
                fallback: false,
            }
        );
    }
}
