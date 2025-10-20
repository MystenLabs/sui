# Nansen Example Indexer

Fetches Sui checkpoints and converts BCS-encoded events to JSON.

## Run

```bash
cargo run
```

## What it does

1. Fetches checkpoint data from Sui's checkpoint store
2. Converts BCS events to human-readable JSON
3. Calculates balance changes for each transaction
4. Prints everything as JSON

## Configuration

Edit these values in `main.rs`:

```rust
let remote_store_url = "https://checkpoints.testnet.sui.io";
let rpc_url = "https://fullnode.testnet.sui.io:443";
let checkpoint_number = 245424622;  // Change to any checkpoint
```

## Example Output

```json
{
  "transaction": { ... },
  "effects": { ... },
  "events": [
    {
      "type_": "0x2::coin::CoinBalance<0x2::sui::SUI>",
      "parsed_json": {
        "balance": "1000000000"
      }
    }
  ],
  "balance_changes": [
    {
      "owner": "0x...",
      "coin_type": "0x2::sui::SUI",
      "amount": -1000000
    }
  ]
}
```