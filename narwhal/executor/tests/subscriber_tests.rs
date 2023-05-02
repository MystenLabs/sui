use anemo::{async_trait, PeerId};
use config::{CommitteeBuilder, Epoch, WorkerCache};
use consensus::consensus_utils::{
    make_certificate_store, make_consensus_store, NUM_SUB_DAGS_PER_SCHEDULE,
};
use consensus::Consensus;
use consensus::NUM_SHUTDOWN_RECEIVERS;
use fastcrypto::hash::Hash;
use network::client::NetworkClient;
use prometheus::Registry;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;
use test_utils::make_optimal_certificates;
use types::{
    Certificate, CommittedSubDag, FetchBatchesRequest, FetchBatchesResponse,
    PreSubscribedBroadcastSender, PrimaryToWorker, WorkerDeleteBatchesMessage,
    WorkerSynchronizeMessage,
};

pub struct PrimaryToWorkerFake {}

#[async_trait]
impl PrimaryToWorker for PrimaryToWorkerFake {
    async fn synchronize(
        &self,
        _request: anemo::Request<WorkerSynchronizeMessage>,
    ) -> Result<anemo::Response<()>, anemo::rpc::Status> {
        Err(anemo::rpc::Status::internal("Unimplemented"))
    }

    async fn fetch_batches(
        &self,
        _request: anemo::Request<FetchBatchesRequest>,
    ) -> Result<anemo::Response<FetchBatchesResponse>, anemo::rpc::Status> {
        Ok(anemo::Response::new(FetchBatchesResponse {
            batches: HashMap::new(),
        }))
    }

    async fn delete_batches(
        &self,
        _request: anemo::Request<WorkerDeleteBatchesMessage>,
    ) -> Result<anemo::Response<()>, anemo::rpc::Status> {
        Err(anemo::rpc::Status::internal("Unimplemented"))
    }
}

#[tokio::test]
async fn test_subscriber_ordering() {
    // create committee
    let fixture = test_utils::CommitteeFixture::builder()
        .randomize_ports(true)
        .build();
    let mut builder = CommitteeBuilder::new(Epoch::default());
    let mut network_clients = Vec::new();
    let mut keys = Vec::new();
    let primary_to_worker_handler = Arc::new(PrimaryToWorkerFake {});
    let mut worker_cache = WorkerCache {
        workers: BTreeMap::new(),
        epoch: 0,
    };

    for (authority, authority_fixture) in fixture.authorities().map(|a| (a.authority(), a)) {
        builder = builder.add_authority(
            authority.protocol_key().clone(),
            authority.stake(),
            authority.primary_address(),
            authority.network_key(),
        );
        keys.push(authority_fixture.id());

        // create network client for each node in the committee
        let network = NetworkClient::new_from_keypair(&authority_fixture.network_keypair());
        for (_, worker_fixture) in &authority_fixture.workers {
            network.set_primary_to_worker_local_handler(
                PeerId(worker_fixture.info().name.0.into()),
                primary_to_worker_handler.clone(),
            );
        }
        network_clients.push(network);
    }
    let committee = builder.build();

    // create certificates
    let genesis = Certificate::genesis(&committee)
        .iter()
        .map(|x| x.digest())
        .collect::<BTreeSet<_>>();
    let (mut certificates, _) = make_optimal_certificates(&committee, 1..=11, &genesis, &keys);

    // run it through consensus to get subdags
    let store = make_consensus_store(&test_utils::temp_dir());
    let metrics = Arc::new(consensus::metrics::ConsensusMetrics::new(&Registry::new()));

    let bullshark = consensus::bullshark::Bullshark::new(
        committee.clone(),
        store.clone(),
        metrics.clone(),
        NUM_SUB_DAGS_PER_SCHEDULE,
    );

    let gc_depth = 50;
    let cert_store = make_certificate_store(&test_utils::temp_dir());
    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);
    let (tx_new_certificates, rx_new_certificates) = test_utils::test_channel!(1);
    let (tx_primary, mut rx_primary) = test_utils::test_channel!(1);
    let (tx_consensus_round_updates, _rx_consensus_round_updates) =
        tokio::sync::watch::channel(consensus::consensus::ConsensusRound::default());
    let (tx_output, mut rx_output) = test_utils::test_channel!(1);

    let _consensus_handle = Consensus::spawn(
        committee,
        gc_depth,
        store,
        cert_store,
        tx_shutdown.subscribe(),
        rx_new_certificates,
        tx_primary,
        tx_consensus_round_updates,
        tx_output,
        bullshark,
        metrics,
    );
    tokio::spawn(async move { while rx_primary.recv().await.is_some() {} });

    tokio::spawn(async move {
        while let Some(certificate) = certificates.pop_front() {
            tx_new_certificates.send(certificate).await.unwrap();
        }
    });

    let mut committed = Vec::new();
    let mut committed_subdags: Vec<CommittedSubDag> = Vec::new();
    for _commit_rounds in 1..=4 {
        let committed_sub_dag: CommittedSubDag = rx_output.recv().await.unwrap();
        committed.extend(committed_sub_dag.certificates.clone());
        committed_subdags.push(committed_sub_dag);
    }

    //create a subscriber
    let authority_fixture = fixture.authorities().next().unwrap();
    let authority_id = authority_fixture.id();

    // let subscriber_handle = spawn_subscriber(
    //     authority_id,
    //     worker_cache,
    //     committee.clone(),
    //     client,
    //     shutdown_receivers,
    //     rx_sequence,
    //     arc_metrics,
    //     restored_consensus_output,
    //     execution_state,
    // );

    // pass in the subdags to the subscriber and check the output ordering
}
