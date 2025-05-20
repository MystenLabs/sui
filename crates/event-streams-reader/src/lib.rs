use anyhow::Result;
use move_core_types::language_storage::StructTag;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{Read, Write};
use sui_light_client::{Proof, ProofTarget, construct_proof};
use sui_rpc_api::Client as RpcClient;
use sui_types::base_types::ObjectID;
use sui_types::dynamic_field::DynamicFieldType;
use sui_types::event::Event;
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::{Identifier, SUI_FRAMEWORK_ADDRESS, TypeTag};

use fastcrypto::hash::Blake2b256;
use fastcrypto::hash::HashFunction;

#[derive(Debug)]
pub struct StreamState {
    pub stream_head: Option<StreamHead>,
    pub completeness_proof: Option<Proof>,
    pub events: Vec<Event>,
}

impl StreamState {
    pub fn new() -> Self {
        Self {
            stream_head: None,
            completeness_proof: None,
            events: Vec::new(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct Field {
    pub id: ObjectID,
    pub name: Key,
    pub value: StreamHead,
}

#[derive(Debug, Deserialize)]
pub struct Key {
    pub address: ObjectID,
    pub ty: Vec<u8>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StreamHead {
    pub root: Vec<u8>,
    pub prev: Vec<u8>,
}

pub fn hash_helper(data: &[u8]) -> Vec<u8> {
    let mut h = Blake2b256::new();
    h.update(data);
    h.finalize().to_vec()
}

pub fn in_stream_commitment(events: &[Event]) -> Vec<u8> {
    let mut event_hashes = Vec::new();
    for event in events.iter() {
        event_hashes.push(hash_helper(&bcs::to_bytes(&event).unwrap()));
    }
    // TODO: Change to a merkle tree
    hash_helper(&bcs::to_bytes(&event_hashes).unwrap())
}

pub fn process_checkpoint_stream_events(
    checkpoint: &CheckpointData,
    accumulator_id: ObjectID,
    stream_id: ObjectID,
    event_type: &str,
    stream_state: &mut StreamState,
) {
    // 1. Check if a relevant tx exists
    let relevant_transaction = checkpoint
        .transactions
        .iter()
        .find(|t| t.output_objects.iter().any(|o| o.id() == accumulator_id));
    if relevant_transaction.is_none() {
        return;
    }

    // 2. Get the stream head object + sanity checks
    let object = relevant_transaction
        .unwrap()
        .output_objects
        .iter()
        .find(|o| o.id() == accumulator_id)
        .unwrap();
    let move_object = object.data.try_as_move().unwrap();
    let type_ = move_object.type_();
    assert!(type_.is_dynamic_field());
    let df_key_type = type_
        .try_extract_field_name(&DynamicFieldType::DynamicField)
        .unwrap();
    let df_value_type = type_.try_extract_field_value().unwrap();
    assert!(
        df_key_type
            == TypeTag::Struct(Box::new(StructTag {
                address: SUI_FRAMEWORK_ADDRESS,
                module: Identifier::new("accumulator").unwrap(),
                name: Identifier::new("Key").unwrap(),
                type_params: vec![],
            }))
    );
    assert!(
        df_value_type
            == TypeTag::Struct(Box::new(StructTag {
                address: SUI_FRAMEWORK_ADDRESS,
                module: Identifier::new("event").unwrap(),
                name: Identifier::new("EventStreamHead").unwrap(),
                type_params: vec![],
            }))
    );

    let cur_stream_head_df = move_object.to_rust::<Field>().unwrap();
    assert_eq!(cur_stream_head_df.name.address, stream_id);

    // 3. Get the events
    let mut matching_events = Vec::new();
    for tx in checkpoint.transactions.iter() {
        for events in tx.events.iter() {
            for event in events.data.iter() {
                if event.type_.to_canonical_string(true) == event_type {
                    matching_events.push(event.clone());
                }
            }
        }
    }

    // 3.5. Authenticate the events against the stream head
    let cur_head = &cur_stream_head_df.value;
    assert_eq!(in_stream_commitment(&matching_events), cur_head.root);
    if stream_state.stream_head.is_some() {
        let prev_head = stream_state.stream_head.as_ref().unwrap();
        let digest = hash_helper(&bcs::to_bytes(prev_head).unwrap());
        assert_eq!(digest, cur_head.prev);
    } else {
        assert_eq!(cur_head.prev, [0; 32]);
    }

    if matching_events.len() > 0 {
        println!("Updated stream with {} events", matching_events.len());
    } else {
        println!("No events found for stream");
    }

    // 4. Construct the proof
    let object_ref = object.compute_object_reference();
    let target = ProofTarget::new().add_object(object_ref, object.clone());
    let object_proof = construct_proof(target, &checkpoint).unwrap();

    // 5. Update the stream state
    stream_state.stream_head = Some(cur_stream_head_df.value);
    stream_state.completeness_proof = Some(object_proof);
    stream_state.events.extend(matching_events);
}

pub fn load_checkpoint(file_path: &str) -> CheckpointData {
    let mut file = File::open(file_path).unwrap();
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).unwrap();
    bcs::from_bytes(&buffer).unwrap()
}

pub async fn download_and_save_checkpoint(checkpoint_number: u64, file_path: &str) -> Result<()> {
    let sui_client = RpcClient::new("http://localhost:9000").unwrap();
    let full_checkpoint = sui_client.get_full_checkpoint(checkpoint_number).await?;
    let mut file = File::create(file_path).unwrap();
    let bytes = bcs::to_bytes(&full_checkpoint).unwrap();
    file.write_all(&bytes).unwrap();
    Ok(())
}
