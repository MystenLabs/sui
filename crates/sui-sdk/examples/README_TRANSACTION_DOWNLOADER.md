# Sui Transaction Downloader

A command-line tool to download and filter Sui blockchain transactions.

## Features

- Download recent transactions from any Sui network (mainnet, testnet, devnet, localnet)
- Filter transactions by status (success/failure)
- Filter transactions by minimum gas cost
- Include detailed transaction data (effects, events, balance changes, object changes)
- Progress tracking during download
- JSON output for easy processing

## Basic Usage

### Download last 1000 transactions (default)

```bash
cargo run --example download_transactions
```

### Download from specific network

```bash
# Mainnet (default)
cargo run --example download_transactions -- --network mainnet

# Testnet
cargo run --example download_transactions -- --network testnet

# Devnet
cargo run --example download_transactions -- --network devnet

# Localnet
cargo run --example download_transactions -- --network localnet
```

### Specify number of transactions and output file

```bash
cargo run --example download_transactions -- \
  --limit 500 \
  --output my_transactions.json
```

## Advanced Options

### Include detailed transaction data

```bash
cargo run --example download_transactions -- \
  --show-effects \
  --show-events \
  --show-balance-changes \
  --show-object-changes \
  --show-input
```

### Use custom RPC endpoint

```bash
cargo run --example download_transactions -- \
  --rpc-url https://your-custom-rpc-endpoint.com
```

## Filtering Examples

> **Note on Filtering**: The Sui RPC API supports server-side filtering by address, object, and move function, but does NOT support filtering by transaction status or gas cost. For status and gas filters, this tool downloads transactions and filters them client-side, which is why you may need to set a `--scan-limit` to control how many transactions are examined.

### Example 1: Download Failed Transactions

Download only transactions that failed during execution:

```bash
cargo run --example download_transactions -- \
  --network mainnet \
  --show-effects \
  --filter-status failure \
  --limit 100 \
  --scan-limit 10000 \
  --output failed_transactions.json
```

**Explanation:**
- `--show-effects`: Required to access transaction status
- `--filter-status failure`: Only include failed transactions
- `--limit 100`: Stop after finding 100 failed transactions
- `--scan-limit 10000`: Scan up to 10,000 transactions to find failures

**Result:**
```json
{
  "digest": "6BU65VWK15TpmKb6VYqYk8mNKcSyYTYvzsqDxH87SJCU",
  "status": "failure",
  "error": "MoveAbort(...) in command 1"
}
```

### Example 2: Download High Gas Cost Transactions

Download transactions with computation cost greater than 100,000:

```bash
cargo run --example download_transactions -- \
  --network mainnet \
  --show-effects \
  --min-gas-cost 100000 \
  --limit 100 \
  --output high_gas_transactions.json
```

**Explanation:**
- `--show-effects`: Required to access gas cost information
- `--min-gas-cost 100000`: Only include transactions with computationCost >= 100,000
- `--limit 100`: Stop after finding 100 matching transactions

**Result:**
All transactions will have `computationCost >= 100000`:
```json
{
  "digest": "2haKr4MajKG3BCKfLwpvdzXyqmjSNWEa2CySjbVyJGoT",
  "gasUsed": "1512000"
}
```

### Example 3: Combine Multiple Filters

Download failed transactions with high gas cost:

```bash
cargo run --example download_transactions -- \
  --network mainnet \
  --show-effects \
  --filter-status failure \
  --min-gas-cost 500000 \
  --limit 50 \
  --scan-limit 50000 \
  --output failed_high_gas.json
```

**Explanation:**
- Combines both status and gas cost filters
- Only transactions that are BOTH failed AND have high gas cost will be included

### Example 4: Analyze Transaction Patterns

Download successful transactions with detailed information:

```bash
cargo run --example download_transactions -- \
  --network mainnet \
  --show-effects \
  --show-events \
  --show-balance-changes \
  --filter-status success \
  --min-gas-cost 1000000 \
  --limit 200 \
  --output complex_transactions.json
```

## Understanding the Output

### Transaction Structure

Each downloaded transaction includes:

```json
{
  "digest": "unique-transaction-id",
  "checkpoint": "checkpoint-number",
  "timestampMs": "timestamp",
  "effects": {
    "status": {
      "status": "success" | "failure",
      "error": "error message (if failed)"
    },
    "gasUsed": {
      "computationCost": "cost-in-gas-units",
      "storageCost": "storage-cost",
      "storageRebate": "storage-rebate",
      "nonRefundableStorageFee": "non-refundable-fee"
    },
    "modifiedAtVersions": [...],
    "sharedObjects": [...],
    "transactionDigest": "...",
    "created": [...],
    "mutated": [...],
    "deleted": [...],
    "gasObject": {...},
    "eventsDigest": "...",
    "dependencies": [...]
  },
  "events": [...],
  "balanceChanges": [...],
  "objectChanges": [...]
}
```

## Processing Downloaded Data

### Using jq to analyze transactions

Count transactions by status:
```bash
cat transactions.json | jq '[.[] | .effects.status.status] | group_by(.) | map({status: .[0], count: length})'
```

Find average gas cost:
```bash
cat transactions.json | jq '[.[] | .effects.gasUsed.computationCost | tonumber] | add / length'
```

Extract unique error types:
```bash
cat failed_transactions.json | jq '[.[] | .effects.status.error] | unique'
```

Find transactions with most events:
```bash
cat transactions.json | jq 'sort_by(.events | length) | reverse | .[0:5] | .[] | {digest, eventCount: (.events | length)}'
```

## Command-Line Options Reference

| Option | Description | Default |
|--------|-------------|---------|
| `--network` | Network to connect to (mainnet, testnet, devnet, localnet) | mainnet |
| `--limit` | Number of matching transactions to download | 1000 |
| `--output` | Output JSON file path | transactions.json |
| `--show-input` | Include transaction input data | false |
| `--show-effects` | Include transaction effects | false |
| `--show-events` | Include emitted events | false |
| `--show-object-changes` | Include object state changes | false |
| `--show-balance-changes` | Include balance changes | false |
| `--rpc-url` | Custom RPC endpoint URL | (network default) |
| `--filter-status` | Filter by status (success, failure) | (no filter) |
| `--min-gas-cost` | Minimum computation cost | (no filter) |
| `--scan-limit` | Maximum transactions to scan when filtering | unlimited |

## Performance Tips

1. **Use `--scan-limit` when filtering**: Prevents scanning the entire chain
2. **Download without filters first**: Then use jq to filter locally if needed
3. **Use smaller `--limit` for testing**: Test filters with small limits first
4. **Parallel downloads**: Run multiple instances with different filters
5. **Network speed**: Testnet/devnet may be faster for testing

## Server-Side Filters (Future Enhancement)

The Sui RPC API supports these server-side filters through `TransactionFilter`:
- `FromAddress` - Filter by sender address
- `ToAddress` - Filter by recipient address
- `InputObject` - Filter by input object ID
- `ChangedObject` - Filter by created/mutated/unwrapped objects
- `AffectedObject` - Filter by any touched object
- `MoveFunction` - Filter by package, module, function calls
- `TransactionKind` - Filter by transaction type

These could be added to the tool as additional command-line options for more efficient filtering. Currently, the tool uses client-side filtering for status and gas cost since those aren't available in the RPC API.

## Troubleshooting

### "Scanned X transactions but only found Y matches"

This is normal when using filters. Increase `--scan-limit` to scan more transactions.

### "Connection timeout"

- Check your network connection
- Try a different network (testnet, devnet)
- Use a custom `--rpc-url` if available

### Large file sizes

- Reduce `--limit`
- Remove unnecessary flags (`--show-events`, etc.)
- Compress the output: `gzip transactions.json`

## Examples Summary

```bash
# 1. Download failed transactions
cargo run --example download_transactions -- \
  --show-effects --filter-status failure --limit 100 --scan-limit 10000

# 2. Download high gas transactions (>100k)
cargo run --example download_transactions -- \
  --show-effects --min-gas-cost 100000 --limit 100

# 3. Download with all details
cargo run --example download_transactions -- \
  --show-effects --show-events --show-balance-changes --limit 500

# 4. Quick test (10 transactions)
cargo run --example download_transactions -- --limit 10
```

## Contributing

To add new filter types:
1. Add command-line argument in `Args` struct
2. Add filter logic in the main download loop
3. Update this README with examples
