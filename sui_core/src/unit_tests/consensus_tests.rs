use super::*;
use crate::authority::authority_tests::get_genesis_package_by_module;
use crate::authority::authority_tests::init_state_with_objects;
use move_core_types::account_address::AccountAddress;
use move_core_types::ident_str;
use rand::rngs::StdRng;
use rand::SeedableRng;
use std::time::Duration;
use sui_adapter::genesis;
use sui_network::transport;
use sui_types::base_types::{ObjectID, TransactionDigest};
use sui_types::crypto::{get_key_pair_from_rng, Signature};
use sui_types::messages::{SignatureAggregator, Transaction, TransactionData};
use sui_types::object::{Data, Object, Owner};
use sui_types::serialize::serialize_cert;
use test_utils::sequencer::Sequencer;

/// Default network buffer size.
const NETWORK_BUFFER_SIZE: usize = 65_000;

#[tokio::test]
async fn handle_consensus_output() {
    let mut rng = StdRng::from_seed([0; 32]);
    let (sender, keypair) = get_key_pair_from_rng(&mut rng);

    // Initialize an authority with a (owned) gas object and a shared object.
    let gas_object_id = ObjectID::random();
    let gas_object = Object::with_id_owner_for_testing(gas_object_id, sender);
    let gas_object_ref = gas_object.compute_object_reference();

    let shared_object_id = ObjectID::random();
    let shared_object = {
        use sui_types::gas_coin::GasCoin;
        use sui_types::object::MoveObject;

        let content = GasCoin::new(shared_object_id, SequenceNumber::new(), 10);
        let data = Data::Move(MoveObject::new(
            /* type */ GasCoin::type_(),
            content.to_bcs_bytes(),
        ));
        Object {
            data,
            owner: Owner::SharedMutable,
            previous_transaction: TransactionDigest::genesis(),
        }
    };

    let authority = init_state_with_objects(vec![gas_object, shared_object]).await;

    // Make a sample transaction.
    let module = "ObjectBasics";
    let function = "create";
    let genesis_package_objects = genesis::clone_genesis_packages();
    let package_object_ref = get_genesis_package_by_module(&genesis_package_objects, module);

    let data = TransactionData::new_move_call(
        sender,
        package_object_ref,
        ident_str!(module).to_owned(),
        ident_str!(function).to_owned(),
        /* type_args */ vec![],
        gas_object_ref,
        /* object_args */ vec![],
        vec![shared_object_id],
        /* pure_args */
        vec![
            16u64.to_le_bytes().to_vec(),
            bcs::to_bytes(&AccountAddress::from(sender)).unwrap(),
        ],
        /* max_gas */ 10_000,
    );
    let signature = Signature::new(&data, &keypair);
    let transaction = Transaction::new(data, signature);

    // Submit the transaction and assemble a certificate.
    let response = authority
        .handle_transaction(transaction.clone())
        .await
        .unwrap();
    let vote = response.signed_transaction.unwrap();
    let certificate = SignatureAggregator::try_new(transaction, &authority.committee)
        .unwrap()
        .append(vote.authority, vote.signature)
        .unwrap()
        .unwrap();
    let serialized_certificate = serialize_cert(&certificate);

    // Spawn a sequencer.
    // TODO [issue #932]: Use a port allocator to avoid port conflicts.
    let consensus_input_address = "127.0.0.1:1309".parse().unwrap();
    let consensus_subscriber_address = "127.0.0.1:1310".parse().unwrap();
    let sequencer = Sequencer {
        input_address: consensus_input_address,
        subscriber_address: consensus_subscriber_address,
        buffer_size: NETWORK_BUFFER_SIZE,
        consensus_delay: Duration::from_millis(0),
    };
    Sequencer::spawn(sequencer).await;

    // Spawn a consensus client.
    let state = Arc::new(authority);
    let consensus_client = ConsensusClient::new(state.clone()).unwrap();
    ConsensusClient::spawn(
        consensus_client,
        consensus_subscriber_address,
        NETWORK_BUFFER_SIZE,
    );

    // Submit a certificate to the sequencer.
    tokio::task::yield_now().await;
    transport::connect(consensus_input_address.to_string(), NETWORK_BUFFER_SIZE)
        .await
        .unwrap()
        .write_data(&serialized_certificate)
        .await
        .unwrap();

    // Wait for the certificate to be processed and ensure the last consensus index
    // has been updated.
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert_eq!(
        state.db().last_consensus_index().unwrap(),
        SequenceNumber::from(1)
    );
}
