use anyhow::Result;
use event_streams_reader::{
    StreamState, download_and_save_checkpoint, load_checkpoint, process_checkpoint_stream_events,
};
use sui_types::base_types::ObjectID;

#[tokio::main]
async fn main() -> Result<()> {
    let stream_id = ObjectID::from_hex_literal(
        "0x233d930d604029457f63c3eabff64234811fc3d9e41df932af695c746efd1342",
    )
    .unwrap();
    let accumulator_id = ObjectID::from_hex_literal(
        "0x1f2f309d9c93b4ef6792ba4a0deb816da52f24a01224554889c5c6838eb632a2",
    )
    .unwrap();
    let event_type = "0xce8459d2b6ed79d56a5f3b469fbed9dc7abcf1ecc7b0b52382b2ac817b95aa49::event_streams_package::AuthEvent".to_string();
    println!("Stream ID: {}", stream_id);
    println!("Accumulator ID: {}", accumulator_id);
    println!("Event type: {}", event_type);

    let mut stream_state = StreamState::new();
    let mut checkpoints_to_be_processed = Vec::new();
    let checkpoints = vec![191, 246, 322];

    for checkpoint in &checkpoints {
        let test_file = format!("test_files/checkpoint-{}.chk", checkpoint);
        if !std::path::Path::new(&test_file).exists() {
            println!("Checkpoint not found, fetching from local network");
            download_and_save_checkpoint(*checkpoint, &test_file).await?;
        }
        let full_checkpoint = load_checkpoint(&test_file);
        checkpoints_to_be_processed.push(full_checkpoint);
    }

    println!("Processing {} checkpoints: {:?}", checkpoints.len(), checkpoints);
    for checkpoint in checkpoints_to_be_processed.iter() {
        process_checkpoint_stream_events(
            checkpoint,
            accumulator_id,
            stream_id,
            &event_type,
            &mut stream_state,
        );
    }
    Ok(())
}
