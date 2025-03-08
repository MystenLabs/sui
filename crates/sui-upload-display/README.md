# Sui Upload Display

A service that extracts and processes display data from Sui blockchain checkpoints and uploads it to Google Cloud Storage (GCS).

## Features
- Processes Sui blockchain checkpoints in batches
- Extracts display data from transaction events
- Tracks and maintains state across epochs
- Uploads display data to Google Cloud Storage as CSV files
- Supports concurrent processing for improved performance

## Usage

To run the service with all options:

```
cargo run -p sui-upload-display -- \
    --gcs-cred-path="/path/to/credentials.json" \
    --gcs-display-bucket="bucket-name" \
    --remote-url="https://checkpoints.mainnet.sui.io" \
    --concurrency-limit=20 \
    --batch-size=200
```

With minimal options (using defaults):

```
cargo run -p sui-upload-display -- \
    --gcs-cred-path="/path/to/credentials.json" \
    --gcs-display-bucket="bucket-name"
```

Get help on available options:

```
cargo run -p sui-upload-display -- --help
```

## Configuration Options

### Command-line Arguments

- `--gcs-cred-path`: Path to Google Cloud Service Account credentials JSON file
- `--gcs-display-bucket`: Name of the Google Cloud Storage bucket to upload files to
- `--remote-url`: URL of the fullnode to fetch checkpoint data from (default: "https://fullnode.mainnet.sui.io:443")
- `--concurrency-limit`: Number of concurrent checkpoints to process (default: 10)
- `--batch-size`: Number of checkpoints to process in one batch (default: 100)

## Implementation Details

The service works as follows:

1. Finds the last processed checkpoint by examining existing files in the GCS bucket
2. Initializes with the epoch data from the latest checkpoint file (if any)
3. Processes batches of checkpoints in the configured batch size
4. For each checkpoint in the batch:
   - Fetches checkpoint data from the Sui fullnode
   - Extracts display update events
   - Stores the updates in memory with their checkpoint and epoch information
5. After processing a batch, updates the in-memory epoch data with the new display entries
6. When an end-of-epoch is detected, uploads the complete display data to GCS
7. Continues to the next batch of checkpoints

The display data is formatted in CSV files with the following columns:
- `object_type`: Type of the object (hex-encoded)
- `id`: Display ID (hex-encoded)
- `version`: Display version
- `bcs`: BCS-encoded display data (hex-encoded)

Files are named with the format `displays_{epoch}_{checkpoint}.csv` 
