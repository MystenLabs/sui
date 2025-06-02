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
use sui_types::committee::Committee;
use sui_types::object::Object;

use fastcrypto::hash::Blake2b256;
use fastcrypto::hash::HashFunction;

#[derive(Debug, Deserialize)]
pub struct StreamHeadField {
    pub id: ObjectID,
    pub name: Key,
    pub value: StreamHead,
}

#[derive(Debug, Deserialize)]
pub struct Key {
    pub address: ObjectID,
    pub ty: Vec<u8>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StreamHead {
    pub root: Vec<u8>,
    pub prev: Vec<u8>,
}

impl From<&Object> for StreamHeadField {
    fn from(object: &Object) -> Self {
        let move_object = object.data.try_as_move().unwrap();

        // Sanity checks on the object type
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
    
        move_object.to_rust::<StreamHeadField>().unwrap()
    }
}

// TODO: Persist to disk both for crash recovery and reducing memory usage
#[derive(Debug)]
pub struct StreamState {
    pub stream_head: Option<StreamHead>,
    pub head_correctness_proof: Option<Proof>,
    // Events batched and ordered by checkpoint number
    pub events: Vec<(u64, Vec<Event>)>,
    // Full checkpoints to be used for proving no events between two checkpoints
    // TODO: Will be replaced with Merkle proofs
    pub full_checkpoints: Vec<CheckpointData>,
}

// An authenticated stream update containing all the events since a given checkpoint
pub struct AuthStreamUpdate<'a> {
    pub start_checkpoint_number: u64,
    pub events: Vec<(u64, Vec<Event>)>,
    pub head_correctness_proof: &'a Proof,
}

// A non-inclusion proof for a stream proving that no events were emitted between two checkpoints
pub struct StreamNonInclusionProof<'a> {
    pub start_checkpoint_number: u64,
    pub end_checkpoint_number: u64,
    pub proof: Vec<&'a CheckpointData>,
}

impl StreamState {
    pub fn new() -> Self {
        Self {
            stream_head: None,
            head_correctness_proof: None,
            events: Vec::new(),
            full_checkpoints: Vec::new(),
        }
    }

    pub fn get_stream_events_since(&self, checkpoint_number: u64) -> Option<AuthStreamUpdate> {
        // First check if we have a completeness proof before proceeding
        let completeness_proof = self.head_correctness_proof.as_ref()?;

        // Find first checkpoint >= requested checkpoint number
        let start_index = self
            .events
            .iter()
            .position(|(c, _)| *c >= checkpoint_number)?;

        // Get all events from start_index onwards
        let events = self.events[start_index..].to_vec();

        Some(AuthStreamUpdate {
            start_checkpoint_number: checkpoint_number,
            events,
            head_correctness_proof: completeness_proof,
        })
    }

    pub fn prove_no_events_between(
        &self,
        start_checkpoint_number: u64,
        end_checkpoint_number: u64,
    ) -> Result<StreamNonInclusionProof> {
        if !self.check_no_events_between(start_checkpoint_number, end_checkpoint_number) {
            return Err(anyhow::anyhow!("Events found between the two checkpoints!"));
        }
        let mut proof = Vec::new();
        for checkpoint in self.full_checkpoints.iter() {
            if checkpoint.checkpoint_summary.sequence_number >= start_checkpoint_number
                && checkpoint.checkpoint_summary.sequence_number <= end_checkpoint_number
            {
                proof.push(checkpoint);
            }
        }
        Ok(StreamNonInclusionProof {
            start_checkpoint_number,
            end_checkpoint_number,
            proof,
        })
    }

    pub fn check_no_events_between(
        &self,
        start_checkpoint_number: u64,
        end_checkpoint_number: u64,
    ) -> bool {
        for (c, _) in self.events.iter() {
            if *c >= start_checkpoint_number && *c <= end_checkpoint_number {
                return false;
            }
        }
        true
    }
}

impl<'a> AuthStreamUpdate<'a> {
    // TODO: Look into the committee parameter
    // TODO: Add tests
    pub fn verify(&self, stream_head: Option<&StreamHead>, committee: &Committee) -> Result<()> {
        let (_, head) = &self.head_correctness_proof.targets.objects[0];
        let new_stream_head_df = StreamHeadField::from(head);

        // 1. Verify the events against stream heads
        verify_multiple_checkpoint_events_against_stream_heads(
            &self.events,
            stream_head,
            &new_stream_head_df.value,
        );

        // 2. Verify the stream head using the object proof
        sui_light_client::verify_proof(committee, &self.head_correctness_proof)?;
        Ok(())
    }
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

pub fn verify_single_checkpoint_events_against_stream_heads(
    events: &[Event],
    old_stream_head: Option<&StreamHead>,
    new_stream_head: &StreamHead,
) {
    let commitment = in_stream_commitment(events);
    assert!(commitment == new_stream_head.root);

    if old_stream_head.is_some() {
        let prev_head = old_stream_head.as_ref().unwrap();
        let digest = hash_helper(&bcs::to_bytes(prev_head).unwrap());
        assert_eq!(digest, new_stream_head.prev);
    } else {
        assert_eq!(new_stream_head.prev, [0; 32]);
    }
}

pub fn verify_multiple_checkpoint_events_against_stream_heads(
    events: &[(u64, Vec<Event>)],
    old_stream_head: Option<&StreamHead>,
    new_stream_head: &StreamHead,
) {
    let mut current_stream_head = old_stream_head;
    for (_, events) in events.iter() {
        verify_single_checkpoint_events_against_stream_heads(
            events,
            current_stream_head,
            new_stream_head,
        );
        current_stream_head = Some(new_stream_head);
    }
}

pub fn process_checkpoint_stream_events(
    checkpoint: &CheckpointData,
    accumulator_id: ObjectID,
    stream_id: ObjectID,
    event_type: &str,
    stream_state: &mut StreamState,
) {
    let is_new_stream = stream_state.stream_head.is_none();
    if !is_new_stream {
        // If it is an old stream...
        assert!(stream_state.full_checkpoints.len() > 0);
        assert!(stream_state.head_correctness_proof.is_some());
        assert!(stream_state.events.len() > 0);
    }

    // 1. Check if a relevant tx exists
    let relevant_transaction = checkpoint
        .transactions
        .iter()
        .find(|t| t.output_objects.iter().any(|o| o.id() == accumulator_id));
    if relevant_transaction.is_none() {
        println!("No relevant transaction found for checkpoint {}", checkpoint.checkpoint_summary.sequence_number);
        return;
    }

    // 2. Get the stream head object + sanity checks
    let object = relevant_transaction
        .unwrap()
        .output_objects
        .iter()
        .find(|o| o.id() == accumulator_id)
        .unwrap();
    let cur_stream_head_df = StreamHeadField::from(object);
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
    verify_single_checkpoint_events_against_stream_heads(
        &matching_events,
        stream_state.stream_head.as_ref(),
        &cur_stream_head_df.value,
    );

    if matching_events.len() > 0 {
        println!("Found {} new event in checkpoint {}", matching_events.len(), checkpoint.checkpoint_summary.sequence_number);
    } else {
        println!("No events found for stream in checkpoint {}", checkpoint.checkpoint_summary.sequence_number);
    }

    // 4. Construct the proof
    let object_ref = object.compute_object_reference();
    let target = ProofTarget::new().add_object(object_ref, object.clone());
    let object_proof = construct_proof(target, &checkpoint).unwrap();

    // 5. Update the stream state
    stream_state.stream_head = Some(cur_stream_head_df.value);
    stream_state.head_correctness_proof = Some(object_proof);
    // Sanity check to ensure that the events are sorted by checkpoint number if present
    if !is_new_stream {
        // If it is an old stream...
        assert!(
            stream_state.events.last().unwrap().0 < checkpoint.checkpoint_summary.sequence_number
        );
    }
    stream_state.events.push((
        checkpoint.checkpoint_summary.sequence_number,
        matching_events,
    ));
    // Sanity check to ensure that the full checkpoints are in sequence
    // if !is_new_stream {
    //     // If it is an old stream...
    //     assert!(
    //         stream_state
    //             .full_checkpoints
    //             .last()
    //             .unwrap()
    //             .checkpoint_summary
    //             .sequence_number
    //             == checkpoint.checkpoint_summary.sequence_number - 1
    //     );
    // }
    stream_state.full_checkpoints.push(checkpoint.clone());
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

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;
    use sui_types::base_types::{ObjectID, SuiAddress};
    use sui_types::event::Event;

    fn create_mock_event(data: u8) -> Event {
        Event {
            package_id: ObjectID::random(),
            transaction_module: Identifier::new("test").unwrap(),
            sender: SuiAddress::from_str(
                "0x65244e117127665b02fed67527c847c5843cd7006c6ca4c0de511e4f41ff98ee",
            )
            .unwrap(),
            type_: StructTag::from_str("0x1::test::TestEvent").unwrap(),
            contents: vec![data, data, data],
        }
    }

    fn create_mock_stream_state_with_events(events: Vec<(u64, Vec<Event>)>) -> StreamState {
        let test_checkpoint = load_checkpoint("test_files/checkpoint.chk");
        let mut state = StreamState::new();
        state.events = events;
        // Mock proof until we have a Proof::default()..
        state.head_correctness_proof = Some(Proof {
            targets: ProofTarget::new(),
            checkpoint_summary: test_checkpoint.checkpoint_summary,
            contents_proof: None,
        });
        state
    }

    #[test]
    fn test_get_stream_events_since_empty_state() {
        let state = StreamState::new();
        assert!(state.get_stream_events_since(1).is_none());
    }

    #[test]
    fn test_get_stream_events_since_no_completeness_proof() {
        let mut state = StreamState::new();
        state.events = vec![(1, vec![create_mock_event(1)])];
        assert!(state.get_stream_events_since(1).is_none());
    }

    #[test]
    fn test_get_stream_events_since_exact_checkpoint() {
        let events = vec![
            (1, vec![create_mock_event(1)]),
            (2, vec![create_mock_event(2)]),
            (3, vec![create_mock_event(3)]),
        ];
        let state = create_mock_stream_state_with_events(events);

        let update = state.get_stream_events_since(2).unwrap();
        assert_eq!(update.start_checkpoint_number, 2);
        assert_eq!(update.events.len(), 2);
        assert_eq!(update.events[0].0, 2);
        assert_eq!(update.events[1].0, 3);
    }

    #[test]
    fn test_get_stream_events_since_between_checkpoints() {
        let events = vec![
            (1, vec![create_mock_event(1)]),
            (3, vec![create_mock_event(3)]),
            (5, vec![create_mock_event(5)]),
        ];
        let state = create_mock_stream_state_with_events(events);

        let update = state.get_stream_events_since(2).unwrap();
        assert_eq!(update.start_checkpoint_number, 2);
        assert_eq!(update.events.len(), 2);
        assert_eq!(update.events[0].0, 3);
        assert_eq!(update.events[1].0, 5);
    }

    #[test]
    fn test_get_stream_events_since_future_checkpoint() {
        let events = vec![
            (1, vec![create_mock_event(1)]),
            (2, vec![create_mock_event(2)]),
        ];
        let state = create_mock_stream_state_with_events(events);

        assert!(state.get_stream_events_since(3).is_none());
    }

    #[test]
    fn test_get_stream_events_since_multiple_events_per_checkpoint() {
        let events = vec![
            (1, vec![create_mock_event(1), create_mock_event(1)]),
            (2, vec![create_mock_event(2), create_mock_event(2)]),
            (3, vec![create_mock_event(3), create_mock_event(3)]),
        ];
        let state = create_mock_stream_state_with_events(events);

        let update = state.get_stream_events_since(2).unwrap();
        assert_eq!(update.start_checkpoint_number, 2);
        assert_eq!(update.events.len(), 2);
        assert_eq!(update.events[0].1.len(), 2);
        assert_eq!(update.events[1].1.len(), 2);
    }
}
