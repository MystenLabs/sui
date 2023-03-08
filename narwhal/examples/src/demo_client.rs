// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::{crate_name, crate_version, App, AppSettings, Arg, SubCommand};
use narwhal::{
    collection_retrieval_result::RetrievalResult, proposer_client::ProposerClient,
    validator_client::ValidatorClient, CertificateDigest, CollectionRetrievalResult, Empty,
    GetCollectionsRequest, GetCollectionsResponse, NodeReadCausalRequest, NodeReadCausalResponse,
    PublicKey, ReadCausalRequest, ReadCausalResponse, RemoveCollectionsRequest, RoundsRequest,
    RoundsResponse,
};
use std::{
    fmt,
    fmt::{Display, Formatter},
};
use tonic::Status;

pub mod narwhal {
    #![allow(clippy::derive_partial_eq_without_eq)]
    tonic::include_proto!("narwhal");
}

/// DEMO CONSTANTS
const PRIMARY_0_PUBLIC_KEY: &str = "Zy82aSpF8QghKE4wWvyIoTWyLetCuUSfk2gxHEtwdbg=";
// Assumption that each transaction costs 1 gas to complete
// Chose this number because it allows demo to complete round + get extra collections when proposing block.
const BLOCK_GAS_LIMIT: i32 = 300000;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let matches = App::new(crate_name!())
        .version(crate_version!())
        .about("A gRPC client emulating the Proposer / Validator API")
        .subcommand(
            SubCommand::with_name("docker_demo")
                .about("run the demo with the hardcoded Docker deployment"),
        )
        .subcommand(
            SubCommand::with_name("run")
                .about("Run the demo with a local gRPC server")
                .arg(
                    Arg::with_name("keys")
                        .long("keys")
                        .help("The base64-encoded publickey of the node to query")
                        .use_delimiter(true)
                        .min_values(2),
                )
                .arg(
                    Arg::with_name("ports")
                        .long("ports")
                        .help("The ports on localhost where to reach the grpc server")
                        .use_delimiter(true)
                        .min_values(2),
                ),
        )
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .get_matches();

    let mut dsts = Vec::new();
    let mut base64_keys = Vec::new();
    match matches.subcommand() {
        Some(("docker_demo", _sub_matches)) => {
            dsts.push("http://127.0.0.1:8000".to_owned());
            base64_keys.push(PRIMARY_0_PUBLIC_KEY.to_owned());
        }
        Some(("run", sub_matches)) => {
            let ports = sub_matches
                .values_of("ports")
                .expect("Invalid ports specified");
            // TODO : check this arg is correctly formatted (number < 65536)
            for port in ports {
                dsts.push(format!("http://127.0.0.1:{port}"))
            }
            let keys = sub_matches
                .values_of("keys")
                .expect("Invalid public keys specified");
            // TODO : check this arg is correctly formatted (pk in base64)
            for key in keys {
                base64_keys.push(key.to_owned())
            }
        }
        _ => unreachable!(),
    }

    println!(
        "******************************** Proposer Service ********************************\n"
    );
    println!("\nConnecting to {} as the proposer.", dsts[0]);
    let mut proposer_client_1 = ProposerClient::connect(dsts[0].clone()).await?;
    let mut validator_client_1 = ValidatorClient::connect(dsts[0].clone()).await?;
    let public_key = base64::decode(&base64_keys[0]).unwrap();

    println!("\n1) Retrieve the range of rounds you have a collection for");
    println!("\n\t---- Use Rounds endpoint ----\n");

    let rounds_request = RoundsRequest {
        public_key: Some(PublicKey {
            bytes: public_key.clone(),
        }),
    };

    println!("\t{}\n", rounds_request);

    let request = tonic::Request::new(rounds_request);
    let response = proposer_client_1.rounds(request).await;
    let rounds_response = response.unwrap().into_inner();

    println!("\t{}\n", rounds_response);

    let oldest_round = rounds_response.oldest_round;
    let newest_round = rounds_response.newest_round;
    let mut round = oldest_round + 1;
    let mut last_completed_round = round;
    let mut proposed_block_gas_cost: i32 = 0;

    println!("\n2) Find collections from earliest round and continue to add collections until gas limit is hit\n");
    let mut block_proposal_collection_ids = Vec::new();
    let mut extra_collections = Vec::new();
    while round <= newest_round {
        let node_read_causal_request = NodeReadCausalRequest {
            public_key: Some(PublicKey {
                bytes: public_key.clone(),
            }),
            round,
        };

        println!("\t-------------------------------------");
        println!("\t| 2a) Find collections for round = {}", round);
        println!("\t-------------------------------------");

        println!("\t{}\n", node_read_causal_request);

        let request = tonic::Request::new(node_read_causal_request);
        let response = proposer_client_1.node_read_causal(request).await;

        if let Some(node_read_causal_response) = println_and_into_inner(response) {
            let mut duplicate_collection_count = 0;
            let mut new_collections = Vec::new();
            let count_of_retrieved_collections = node_read_causal_response.collection_ids.len();
            for collection_id in node_read_causal_response.collection_ids {
                if block_proposal_collection_ids.contains(&collection_id) {
                    duplicate_collection_count += 1;
                } else {
                    println!(
                        "\n\t\t2b) Get collection [{}] payloads to calculate gas cost of proposed block.\n", collection_id
                    );

                    let get_collections_request = GetCollectionsRequest {
                        collection_ids: vec![collection_id.clone()],
                    };

                    println!("\t\t{}\n", get_collections_request);

                    let request = tonic::Request::new(get_collections_request);
                    let response = validator_client_1.get_collections(request).await;
                    let get_collection_response = response.unwrap().into_inner();

                    let (total_num_of_transactions, total_transactions_size) =
                        get_total_transaction_count_and_size(
                            get_collection_response.result.clone(),
                        );

                    // TODO: This doesn't work in Docker yet, figure out why
                    println!("\t\tFound {total_num_of_transactions} transactions with a total size of {total_transactions_size} bytes");

                    proposed_block_gas_cost += total_num_of_transactions;
                    if proposed_block_gas_cost <= BLOCK_GAS_LIMIT {
                        println!("\t\tAdding {total_num_of_transactions} transactions to the proposed block, increasing the block gas cost to {proposed_block_gas_cost}");
                        new_collections.push(collection_id);
                    } else {
                        println!("\t\t*Not adding {total_num_of_transactions} transactions to the proposed block as it would increase the block gas cost to {proposed_block_gas_cost} which is greater than block gas limit of {BLOCK_GAS_LIMIT}");
                        break;
                    }
                }
            }
            if new_collections.len() + duplicate_collection_count != count_of_retrieved_collections
            {
                println!(
                    "\t\tWe added {} extra collections to the block proposal from round {round}",
                    new_collections.len()
                );
                extra_collections.extend(new_collections.clone());
                last_completed_round = round - 1;
            }

            block_proposal_collection_ids.extend(new_collections);

            println!("\t\tDeduped {:?} collections\n", duplicate_collection_count);
        } else {
            println!("\tError trying to node read causal at round {round}\n")
        }
        if proposed_block_gas_cost >= BLOCK_GAS_LIMIT {
            println!("\t\t***********************************************************************");
            println!(
                "\t\t* Will not continue on more rounds as gas limit {} has been reached",
                BLOCK_GAS_LIMIT
            );
            println!("\t\t***********************************************************************");
            break;
        } else {
            round += 1;
        }
    }

    if round > newest_round {
        last_completed_round = newest_round;
    }

    println!(
        "\n2c) Find the first collection returned from node read causal for fully completed round {last_completed_round} before gas limit was reached.\n"
    );
    println!("---- Use NodeReadCausal endpoint ----\n");

    let mut block_proposal_starting_collection: Option<CertificateDigest> = None;

    while block_proposal_starting_collection.is_none() {
        let node_read_causal_request = NodeReadCausalRequest {
            public_key: Some(PublicKey {
                bytes: public_key.clone(),
            }),
            round: last_completed_round,
        };

        println!("\t{}\n", node_read_causal_request);

        let request = tonic::Request::new(node_read_causal_request);
        let response = proposer_client_1.node_read_causal(request).await;

        if let Some(node_read_causal_response) = println_and_into_inner(response) {
            block_proposal_starting_collection =
                Some(node_read_causal_response.collection_ids[0].clone());
            // NodeReadCausal here will return the expected order of collections for validation versus the deduping we did in our search above.
            block_proposal_collection_ids = node_read_causal_response.collection_ids.clone();
            block_proposal_collection_ids.extend(extra_collections.clone());
        } else {
            println!("\tError trying to node read causal at round {last_completed_round} going back another round and retrying...\n");
            last_completed_round -= 1;
        }
    }

    let block_proposal_starting_collection = block_proposal_starting_collection.unwrap();

    println!(
        "\n\tProposing a block with {} collections starting from collection {block_proposal_starting_collection}!\n",
        block_proposal_collection_ids.len()
    );

    println!(
        "\tBroadcasting block proposal with starting certificate `H` {block_proposal_starting_collection} from round {last_completed_round} in DAG + {} extra collections that fit in the block proposal.", extra_collections.len()
    );
    println!("\tValidators should call ReadCausal(H) which will return [collections] and call GetCollections([collections] + [{} extra_collections]) ", extra_collections.len());

    println!(
        "\n******************************** Validator Service ********************************\n"
    );
    let other_validator = if dsts.len() > 1 {
        dsts[1].clone()
    } else {
        // we're probably running the docker command with a single endpoint
        dsts[0].clone()
    };
    println!("\nConnecting to {other_validator} as the validator");
    let mut validator_client_2 = ValidatorClient::connect(other_validator).await?;

    println!(
        "\n3) Find all causal collections from the starting collection {block_proposal_starting_collection} in block proposal.\n",
    );
    println!("\n\t---- Use ReadCausal endpoint ----\n");

    let mut block_validation_collection_ids = Vec::new();
    let read_causal_request = ReadCausalRequest {
        collection_id: Some(block_proposal_starting_collection),
    };

    println!("\t{}\n", read_causal_request);

    let request = tonic::Request::new(read_causal_request);
    let response = validator_client_2.read_causal(request).await;
    let read_causal_response = response.unwrap().into_inner();

    println!("\t{}\n", read_causal_response);

    block_validation_collection_ids.extend(read_causal_response.collection_ids);

    println!("\tFound {} collections from read causal which will be combined with the {} extra collections from the block proposal\n", block_validation_collection_ids.len(), extra_collections.len());

    block_validation_collection_ids.extend(extra_collections);

    println!("\tProposed block included the following collections before compressing proposal:\n");
    let mut result = "\t\t*** Block Proposal Collections ***".to_string();
    for id in block_proposal_collection_ids.clone() {
        result = format!("{}\n\t\t|-id=\"{}\"", result, id);
    }
    println!("{}", result);

    println!(
        "\n\tBlock validation found the following collections after decompressing proposal:\n"
    );
    let mut result = "\t\t*** Block Validation Collections ***".to_string();
    for id in block_validation_collection_ids.clone() {
        result = format!("{}\n\t\t|-id=\"{}\"", result, id);
    }
    println!("{}", result);

    // We're comparing `block_validation_collection_ids` which is a validator determined artifact,
    // and `block_proposal_collection_ids` which is a proposer artifact. In production, the
    // consensus would not have access to that second artifact at any node but the proposer,
    // we are only show this here for didactic purposes.
    if block_proposal_collection_ids == block_validation_collection_ids {
        println!("\n\tThey match in value and order! Moving on to find the transactions...\n");
    } else {
        println!("\n\tThey dont match! Aborting...\n");
        return Ok(());
    }

    println!(
        "\n4) Obtain the data payload for {} collections in block proposal.\n",
        block_validation_collection_ids.len()
    );

    println!("\n\t---- Use GetCollections endpoint ----\n");

    let get_collections_request = GetCollectionsRequest {
        collection_ids: block_validation_collection_ids.clone(),
    };

    println!("\t{}\n", get_collections_request);

    let request = tonic::Request::new(get_collections_request);
    let response = validator_client_2.get_collections(request).await;
    let get_collection_response = response.unwrap().into_inner();

    println!("\t{}\n", get_collection_response);

    let (total_num_of_transactions, total_transactions_size) =
        get_total_transaction_count_and_size(get_collection_response.result.clone());

    // TODO: This doesn't work in Docker yet, figure out why
    println!("\tFound {total_num_of_transactions} transactions with a total size of {total_transactions_size} bytes\n");

    println!("\tWaiting for validators to decide whether to vote for the block...\n");
    println!("\tVote completed successfully, block can be removed!\n");

    println!("\n4) Remove collections that have been voted on and committed.\n");
    println!("\n\t---- Test RemoveCollections endpoint ----\n");

    let remove_collections_request = RemoveCollectionsRequest {
        collection_ids: block_validation_collection_ids.clone(),
    };

    println!("\t{}\n", remove_collections_request);

    let request = tonic::Request::new(remove_collections_request);
    let response = validator_client_2.remove_collections(request).await;
    if response.is_ok() {
        println!("\tSuccessfully removed committed collections\n");
    } else {
        println!("\tWas not able to remove committed collections\n");
    }

    Ok(())
}

fn get_total_transaction_count_and_size(result: Vec<CollectionRetrievalResult>) -> (i32, usize) {
    let mut total_num_of_transactions = 0;
    let mut total_transactions_size = 0;
    for r in result {
        match r.retrieval_result.unwrap() {
            RetrievalResult::Collection(collection) => {
                for t in collection.transactions {
                    total_transactions_size += t.transaction.len();
                    total_num_of_transactions += 1;
                }
            }
            RetrievalResult::Error(_) => {}
        }
    }
    (total_num_of_transactions, total_transactions_size)
}

////////////////////////////////////////////////////////////////////////
/// Formatting the requests and responses                             //
////////////////////////////////////////////////////////////////////////
impl Display for GetCollectionsRequest {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut result = "*** GetCollectionsRequest ***".to_string();
        for id in &self.collection_ids {
            result = format!("{}\n\t\t|-id=\"{}\"", result, id);
        }
        write!(f, "{}", result)
    }
}

impl Display for RemoveCollectionsRequest {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut result = "*** RemoveCollectionsRequest ***".to_string();
        for id in &self.collection_ids {
            result = format!("{}\n\t|-id=\"{}\"", result, id);
        }
        write!(f, "{}", result)
    }
}

impl Display for Empty {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let result = "*** Empty ***".to_string();
        write!(f, "{}", result)
    }
}

impl Display for GetCollectionsResponse {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut result = "*** GetCollectionsResponse ***".to_string();

        for r in self.result.clone() {
            match r.retrieval_result.unwrap() {
                RetrievalResult::Collection(collection) => {
                    let collection_id = &collection.id.unwrap();
                    let mut transactions_size = 0;
                    let mut num_of_transactions = 0;

                    for t in collection.transactions {
                        transactions_size += t.transaction.len();
                        num_of_transactions += 1;
                    }

                    result = format!(
                        "{}\n\t|-Collection id {}, transactions {}, size: {} bytes",
                        result, collection_id, num_of_transactions, transactions_size
                    );
                }
                RetrievalResult::Error(error) => {
                    result = format!(
                        "{}\n\tError for certificate id {}, error: {}",
                        result,
                        &error.id.unwrap(),
                        error.error
                    );
                }
            }
        }

        write!(f, "{}", result)
    }
}

impl Display for NodeReadCausalResponse {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut result = "*** NodeReadCausalResponse ***".to_string();

        for id in &self.collection_ids {
            result = format!("{}\n\t|-id=\"{}\"", result, id);
        }

        write!(f, "{}", result)
    }
}

impl Display for NodeReadCausalRequest {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut result = "**** NodeReadCausalRequest ***".to_string();

        result = format!("{}\n\t|-Request for round {}", result, &self.round);
        result = format!(
            "{}\n\t|-Authority: {}",
            result,
            base64::encode(self.public_key.clone().unwrap().bytes)
        );

        write!(f, "{}", result)
    }
}

impl Display for ReadCausalResponse {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut result = "*** ReadCausalResponse ***".to_string();

        for id in &self.collection_ids {
            result = format!("{}\n\tid=\"{}\"", result, id);
        }

        write!(f, "{}", result)
    }
}

impl Display for ReadCausalRequest {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut result = "**** ReadCausalRequest ***".to_string();

        result = format!(
            "{}\n\t|-Request for collection {}",
            result,
            &self.collection_id.as_ref().unwrap()
        );

        write!(f, "{}", result)
    }
}

impl Display for RoundsRequest {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut result = "**** RoundsRequest ***".to_string();

        result = format!(
            "{}\n\t|-Authority: {}",
            result,
            base64::encode(self.public_key.clone().unwrap().bytes)
        );

        write!(f, "{}", result)
    }
}

impl Display for RoundsResponse {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut result = "**** RoundsResponse ***".to_string();
        result = format!(
            "{}\n\t|-oldest_round: {}, newest_round: {}",
            result, &self.oldest_round, &self.newest_round
        );

        write!(f, "{}", result)
    }
}

impl Display for CertificateDigest {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", base64::encode(&self.digest))
    }
}

fn println_and_into_inner<T>(result: Result<tonic::Response<T>, Status>) -> Option<T>
where
    T: Display,
{
    match result {
        Ok(response) => {
            let inner = response.into_inner();
            println!("\t{}", &inner);
            Some(inner)
        }
        Err(error) => {
            println!("\t{:?}", error);
            None
        }
    }
}
