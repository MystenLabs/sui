// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt::{Display, Formatter};
use std::net::SocketAddr;
use std::str::FromStr;

use fastcrypto::encoding::{Encoding, Hex};
use reqwest::Client;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::task::JoinHandle;

use sui_config::local_ip_utils;
use sui_keys::keystore::AccountKeystore;
use sui_keys::keystore::Keystore;
use sui_rosetta::operations::Operations;
use sui_rosetta::types::{
    AccountBalanceRequest, AccountBalanceResponse, AccountIdentifier, ConstructionCombineRequest,
    ConstructionCombineResponse, ConstructionMetadataRequest, ConstructionMetadataResponse,
    ConstructionPayloadsRequest, ConstructionPayloadsResponse, ConstructionPreprocessRequest,
    ConstructionPreprocessResponse, ConstructionSubmitRequest, Currencies, NetworkIdentifier,
    PreprocessMetadata, Signature, SignatureType, SubAccount, SubAccountType, SuiEnv,
    TransactionIdentifierResponse,
};
use sui_rosetta::{RosettaOfflineServer, RosettaOnlineServer};
use sui_sdk::SuiClient;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::SuiSignature;

pub async fn start_rosetta_test_server(client: SuiClient) -> (RosettaClient, Vec<JoinHandle<()>>) {
    let online_server = RosettaOnlineServer::new(SuiEnv::LocalNet, client);
    let offline_server = RosettaOfflineServer::new(SuiEnv::LocalNet);
    let local_ip = local_ip_utils::localhost_for_testing();
    let port = local_ip_utils::get_available_port(&local_ip);
    let rosetta_address = format!("{}:{}", local_ip, port);
    let online_handle = tokio::spawn(async move {
        online_server
            .serve(SocketAddr::from_str(&rosetta_address).unwrap())
            .await
    });
    let offline_port = local_ip_utils::get_available_port(&local_ip);
    let offline_address = format!("{}:{}", local_ip, offline_port);
    let offline_handle = tokio::spawn(async move {
        offline_server
            .serve(SocketAddr::from_str(&offline_address).unwrap())
            .await
    });

    // allow rosetta to process the genesis block.
    tokio::task::yield_now().await;
    (
        RosettaClient::new(port, offline_port),
        vec![online_handle, offline_handle],
    )
}

#[derive(Deserialize, PartialEq, Debug)]
pub struct RosettaError {
    pub code: i32,
    pub message: String,
    pub description: Option<String>,
    pub retriable: bool,
    pub details: Option<serde_json::Value>,
}

#[derive(Default, Debug)]
pub struct FlowResponses {
    pub preprocess: Option<Result<ConstructionPreprocessResponse, RosettaError>>,
    pub metadata: Option<Result<ConstructionMetadataResponse, RosettaError>>,
    pub payloads: Option<Result<ConstructionPayloadsResponse, RosettaError>>,
    pub combine: Option<Result<ConstructionCombineResponse, RosettaError>>,
    pub submit: Option<Result<TransactionIdentifierResponse, RosettaError>>,
}

#[derive(Deserialize, Debug)]
enum RosettaAPIResult<T> {
    #[serde(untagged)]
    Ok(T),
    #[serde(untagged)]
    Err(RosettaError),
}

impl<T> From<RosettaAPIResult<T>> for Result<T, RosettaError> {
    fn from(val: RosettaAPIResult<T>) -> Self {
        match val {
            RosettaAPIResult::Ok(ok) => Ok(ok),
            RosettaAPIResult::Err(e) => Err(e),
        }
    }
}

pub struct RosettaClient {
    client: Client,
    online_port: u16,
    offline_port: u16,
}

impl RosettaClient {
    fn new(online: u16, offline: u16) -> Self {
        let client = Client::new();
        Self {
            client,
            online_port: online,
            offline_port: offline,
        }
    }

    // Used to print port, when keeping test running by waiting for online server handle.
    #[allow(dead_code)]
    pub fn online_port(&self) -> u16 {
        self.online_port
    }

    pub async fn call<R: Serialize, T: DeserializeOwned>(
        &self,
        endpoint: RosettaEndpoint,
        request: &R,
    ) -> Result<T, RosettaError> {
        let port = if endpoint.online() {
            self.online_port
        } else {
            self.offline_port
        };
        let response = self
            .client
            .post(format!("http://127.0.0.1:{port}/{endpoint}"))
            .json(&serde_json::to_value(request).unwrap())
            .send()
            .await
            .unwrap();
        let json: Value = response.json().await.unwrap();
        if let Ok(v) = serde_json::from_value::<RosettaAPIResult<T>>(json.clone()) {
            v.into()
        } else {
            panic!("Failed to deserialize json value: {json:#?}")
        }
    }

    /// rosetta construction e2e flow, see https://www.rosetta-api.org/docs/flow.html#construction-api
    pub async fn rosetta_flow(
        &self,
        operations: &Operations,
        keystore: &Keystore,
        metadata: Option<PreprocessMetadata>,
    ) -> FlowResponses {
        let network_identifier = NetworkIdentifier {
            blockchain: "sui".to_string(),
            network: SuiEnv::LocalNet,
        };
        let mut resps = FlowResponses::default();
        // Preprocess
        let preprocess = self
            .call(
                RosettaEndpoint::Preprocess,
                &ConstructionPreprocessRequest {
                    network_identifier: network_identifier.clone(),
                    operations: operations.clone(),
                    metadata,
                },
            )
            .await;
        resps.preprocess = Some(preprocess);
        let Ok(preprocess) = &resps.preprocess.as_ref().unwrap() else {
            return resps;
        };
        println!("Preprocess : {preprocess:?}");
        // Metadata
        let metadata = self
            .call(
                RosettaEndpoint::Metadata,
                &ConstructionMetadataRequest {
                    network_identifier: network_identifier.clone(),
                    options: preprocess.options.clone(),
                    public_keys: vec![],
                },
            )
            .await;
        resps.metadata = Some(metadata);
        let Ok(metadata) = &resps.metadata.as_ref().unwrap() else {
            return resps;
        };

        println!("Metadata : {metadata:?}");
        // Payload
        let payloads = self
            .call(
                RosettaEndpoint::Payloads,
                &ConstructionPayloadsRequest {
                    network_identifier: network_identifier.clone(),
                    operations: operations.clone(),
                    metadata: Some(metadata.metadata.clone()),
                    public_keys: vec![],
                },
            )
            .await;
        resps.payloads = Some(payloads);
        let Ok(payloads) = resps.payloads.as_ref().unwrap() else {
            return resps;
        };
        println!("Payload : {payloads:?}");
        // Combine
        let signing_payload = payloads.payloads.first().unwrap();
        let bytes = Hex::decode(&signing_payload.hex_bytes).unwrap();
        let signer = signing_payload.account_identifier.address;
        let signature = keystore.sign_hashed(&signer, &bytes).await.unwrap();
        let public_key = keystore.export(&signer).unwrap().public();
        let combine: Result<ConstructionCombineResponse, RosettaError> = self
            .call(
                RosettaEndpoint::Combine,
                &ConstructionCombineRequest {
                    network_identifier: network_identifier.clone(),
                    unsigned_transaction: payloads.unsigned_transaction.clone(),
                    signatures: vec![Signature {
                        signing_payload: signing_payload.clone(),
                        public_key: public_key.into(),
                        signature_type: SignatureType::Ed25519,
                        hex_bytes: Hex::from_bytes(SuiSignature::signature_bytes(&signature)),
                    }],
                },
            )
            .await;
        resps.combine = Some(combine);
        let Ok(combine) = resps.combine.as_ref().unwrap() else {
            return resps;
        };
        println!("Combine : {combine:?}");
        // Submit
        let submit = self
            .call(
                RosettaEndpoint::Submit,
                &ConstructionSubmitRequest {
                    network_identifier,
                    signed_transaction: combine.signed_transaction.clone(),
                },
            )
            .await;
        resps.submit = Some(submit);
        resps
    }

    pub async fn get_balance(
        &self,
        network_identifier: NetworkIdentifier,
        address: SuiAddress,
        sub_account: Option<SubAccountType>,
    ) -> AccountBalanceResponse {
        let sub_account = sub_account.map(|account_type| SubAccount { account_type });
        let request = AccountBalanceRequest {
            network_identifier,
            account_identifier: AccountIdentifier {
                address,
                sub_account,
            },
            block_identifier: Default::default(),
            currencies: Currencies(vec![]),
        };
        self.call(RosettaEndpoint::Balance, &request).await.unwrap()
    }
}

#[allow(dead_code)]
pub enum RosettaEndpoint {
    Derive,
    Payloads,
    Combine,
    Preprocess,
    Hash,
    Parse,
    List,
    Options,
    Block,
    Balance,
    Coins,
    Transaction,
    Submit,
    Metadata,
    Status,
}

impl RosettaEndpoint {
    pub fn endpoint(&self) -> &str {
        match self {
            RosettaEndpoint::Derive => "construction/derive",
            RosettaEndpoint::Payloads => "construction/payloads",
            RosettaEndpoint::Combine => "construction/combine",
            RosettaEndpoint::Preprocess => "construction/preprocess",
            RosettaEndpoint::Hash => "construction/hash",
            RosettaEndpoint::Parse => "construction/parse",
            RosettaEndpoint::List => "network/list",
            RosettaEndpoint::Options => "network/options",
            RosettaEndpoint::Block => "block",
            RosettaEndpoint::Balance => "account/balance",
            RosettaEndpoint::Coins => "account/coins",
            RosettaEndpoint::Transaction => "block/transaction",
            RosettaEndpoint::Submit => "construction/submit",
            RosettaEndpoint::Metadata => "construction/metadata",
            RosettaEndpoint::Status => "network/status",
        }
    }

    pub fn online(&self) -> bool {
        match self {
            RosettaEndpoint::Derive
            | RosettaEndpoint::Payloads
            | RosettaEndpoint::Combine
            | RosettaEndpoint::Preprocess
            | RosettaEndpoint::Hash
            | RosettaEndpoint::Parse
            | RosettaEndpoint::List
            | RosettaEndpoint::Options => false,
            RosettaEndpoint::Block
            | RosettaEndpoint::Balance
            | RosettaEndpoint::Coins
            | RosettaEndpoint::Transaction
            | RosettaEndpoint::Submit
            | RosettaEndpoint::Metadata
            | RosettaEndpoint::Status => true,
        }
    }
}

impl Display for RosettaEndpoint {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.endpoint())
    }
}
