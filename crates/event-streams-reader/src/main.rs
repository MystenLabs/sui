use anyhow::Result;
use event_streams_reader::{
    StreamState, download_and_save_checkpoint, load_checkpoint, process_checkpoint_stream_events,
};
use sui_types::base_types::ObjectID;

#[tokio::main]
async fn main() -> Result<()> {
    let stream_id = ObjectID::from_hex_literal(
        "0xfc92e98d019918057430d2916936c63604e7bc25ac3f1ff5305c34203f6266e8",
    )
    .unwrap();
    let accumulator_id = ObjectID::from_hex_literal(
        "0x01ad79f1d96e21c362e6cda62beb63831ec787206957a2a1018983c45692d057",
    )
    .unwrap();
    let event_type = "0x29d00102a2398fb49e95ffb9066c6cff23f161ab815cab8dd3c2729de919708f::event_streams_package::AuthEvent".to_string();

    let test_file = "test_files/checkpoint.chk";
    let mut stream_state = StreamState::new();
    let mut checkpoints_to_be_processed = Vec::new();

    if !std::path::Path::new(test_file).exists() {
        println!("Checkpoint not found, fetching from local network");
        download_and_save_checkpoint(202, test_file).await?;
    }

    println!("Loading checkpoint from file");
    let full_checkpoint = load_checkpoint(test_file);
    // TODO: Add more
    checkpoints_to_be_processed.push(full_checkpoint);

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
