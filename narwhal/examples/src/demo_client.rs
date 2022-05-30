#[allow(unused_imports)]
use narwhal::configuration_client::ConfigurationClient;
use narwhal::proposer_client::ProposerClient;
use narwhal::validator_client::ValidatorClient;
#[allow(unused_imports)]
use narwhal::{
    CertificateDigest, GetCollectionsRequest, MultiAddr, NewNetworkInfoRequest,
    NodeReadCausalRequest, PublicKey, ReadCausalRequest, RemoveCollectionsRequest, RoundsRequest,
    ValidatorData,
};

pub mod narwhal {
    tonic::include_proto!("narwhal");
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /*

    println!("\n******************************** Configuration Service ********************************\n");
    let mut client = ConfigurationClient::connect("http://127.0.0.1:8000").await?;
    let stake_weight = 1;
    let address = MultiAddr {
        address: "/ip4/127.0.0.1".to_string(),
    };

    println!("\n---- Test NewNetworkInfo endpoint ----\n");
    let request = tonic::Request::new(NewNetworkInfoRequest {
        epoch_number: 0,
        validators: vec![ValidatorData {
            public_key: Some(PublicKey {
                bytes: public_key.clone(),
            }),
            stake_weight,
            address: Some(address),
        }],
    });

    let response = client.new_network_info(request).await;

    println!("NewNetworkInfoResponse={:?}", response);

    */

    println!(
        "\n******************************** Proposer Service ********************************\n"
    );
    let mut client = ProposerClient::connect("http://127.0.0.1:8000").await?;
    let public_key = base64::decode("Zy82aSpF8QghKE4wWvyIoTWyLetCuUSfk2gxHEtwdbg=").unwrap();
    let gas_limit = 10;

    println!("\n1) Retrieve the range of rounds you have a collection for\n");
    println!("\n---- Use Rounds endpoint ----\n");

    let request = tonic::Request::new(RoundsRequest {
        public_key: Some(PublicKey {
            bytes: public_key.clone(),
        }),
    });

    println!("RoundsRequest={:?}\n", request);

    let response = client.rounds(request).await;

    println!("RoundsResponse={:?}\n", response);

    let rounds_response = response.unwrap().into_inner();
    let oldest_round = rounds_response.oldest_round;
    let newest_round = rounds_response.newest_round;
    let mut round = oldest_round + 1;
    println!("\n2) Find collections from earliest round and continue to add collections until gas limit is hit\n");
    println!("\n---- Use NodeReadCausal endpoint ----\n");

    let mut collection_ids: Vec<CertificateDigest> = vec![];
    while round < newest_round && collection_ids.len() < gas_limit {
        let request = tonic::Request::new(NodeReadCausalRequest {
            public_key: Some(PublicKey {
                bytes: public_key.clone(),
            }),
            round,
        });

        println!("NodeReadCausalRequest={:?}\n", request);

        let response = client.node_read_causal(request).await;

        println!("NodeReadCausalResponse={:?}\n", response);

        let node_read_causal_response = response.unwrap().into_inner();

        if collection_ids.len() + node_read_causal_response.collection_ids.len() <= gas_limit {
            collection_ids.extend(node_read_causal_response.collection_ids);
        } else {
            println!("Reached gas limit of {gas_limit}, stopping search for more collections\n");
            break;
        }
        round += 1;
    }

    println!(
        "Proposing block with {} collections!\n",
        collection_ids.len()
    );

    println!(
        "\n******************************** Validator Service ********************************\n"
    );
    let mut client = ValidatorClient::connect("http://127.0.0.1:8000").await?;

    println!("\n3) Find all causal collections from the collections found.\n");
    println!("\n---- Use ReadCausal endpoint ----\n");
    let node_read_causal_cids = collection_ids.clone();
    for collection_id in node_read_causal_cids {
        let request = tonic::Request::new(ReadCausalRequest {
            collection_id: Some(collection_id),
        });

        println!("ReadCausalRequest={:?}\n", request);

        let response = client.read_causal(request).await;

        println!("ReadCausalResponse={:?}\n", response);

        let read_causal_response = response.unwrap().into_inner();

        collection_ids.extend(read_causal_response.collection_ids);
    }

    println!("\n4) Obtain the data payload from collections found.\n");
    println!("\n---- Use GetCollections endpoint ----\n");
    let request = tonic::Request::new(GetCollectionsRequest {
        collection_ids: collection_ids.clone(),
    });

    println!("GetCollectionsRequest={:?}\n", request);

    let response = client.get_collections(request).await;

    println!("GetCollectionsResponse={:?}\n", response);

    let get_collection_response = response.unwrap().into_inner();

    // TODO: This doesn't work yet, figure out why workers are crashing.
    println!("Found {} batches", get_collection_response.result.len());

    println!("\n4) Remove collections that have been voted on and committed.\n");
    println!("\n---- Test RemoveCollections endpoint ----\n");
    let request = tonic::Request::new(RemoveCollectionsRequest { collection_ids });

    let response = client.remove_collections(request).await;

    println!("RemoveCollectionsResponse={:?}", response);

    Ok(())
}
