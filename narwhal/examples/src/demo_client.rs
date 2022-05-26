use narwhal::configuration_client::ConfigurationClient;
use narwhal::proposer_client::ProposerClient;
use narwhal::validator_client::ValidatorClient;
use narwhal::{
    CertificateDigest, GetCollectionsRequest, MultiAddr, NewNetworkInfoRequest,
    NodeReadCausalRequest, PublicKey, ReadCausalRequest, RemoveCollectionsRequest, RoundsRequest,
    ValidatorData,
};

use base64;

pub mod narwhal {
    tonic::include_proto!("narwhal");
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n******************************** Data Setup ********************************\n");

    println!("\n---- TODO ----\n");

    println!(
        "\n******************************** Proposer Service ********************************\n"
    );
    let mut client = ProposerClient::connect("http://127.0.0.1:8000").await?;
    let public_key = base64::decode("Zy82aSpF8QghKE4wWvyIoTWyLetCuUSfk2gxHEtwdbg=").unwrap();

    println!("\n---- Test Rounds endpoint ----\n");
    let request = tonic::Request::new(RoundsRequest {
        public_key: Some(PublicKey {
            bytes: public_key.clone(),
        }),
    });

    let response = client.rounds(request).await;

    println!("RoundsResponse={:?}", response);

    println!("\n---- Test NodeReadCausal endpoint ----\n");
    let request = tonic::Request::new(NodeReadCausalRequest {
        public_key: Some(PublicKey {
            bytes: public_key.clone(),
        }),
        round: 0,
    });

    let response = client.node_read_causal(request).await;

    println!("NodeReadCausalResponse={:?}", response);

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

    println!(
        "\n******************************** Validator Service ********************************\n"
    );
    let mut client = ValidatorClient::connect("http://127.0.0.1:8000").await?;
    let collection_id = CertificateDigest {
        digest: vec![
            81, 117, 143, 158, 196, 159, 127, 131, 22, 151, 162, 131, 187, 140, 130, 177, 44, 127,
            128, 53, 183, 25, 33, 177, 89, 8, 46, 93, 150, 44, 230, 9,
        ],
    };

    println!("\n---- Test GetCollections endpoint ----\n");
    let request = tonic::Request::new(GetCollectionsRequest {
        collection_ids: vec![collection_id.clone()],
    });

    let response = client.get_collections(request).await;

    println!("GetCollectionsResponse={:?}", response);

    println!("\n---- Test ReadCausal endpoint ----\n");
    let request = tonic::Request::new(ReadCausalRequest {
        collection_id: Some(collection_id.clone()),
    });

    let response = client.read_causal(request).await;

    println!("ReadCausalResponse={:?}", response);

    println!("\n---- Test RemoveCollections endpoint ----\n");
    let request = tonic::Request::new(RemoveCollectionsRequest {
        collection_ids: vec![collection_id.clone()],
    });

    let response = client.remove_collections(request).await;

    println!("RemoveCollectionsResponse={:?}", response);

    Ok(())
}
