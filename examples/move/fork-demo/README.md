# Fork Demo - Checkpoint Fork Testing Example

This example demonstrates how to use Sui's checkpoint fork testing feature, which allows you to run Move tests against real blockchain state at a specific checkpoint, similar to Foundry's fork testing in Ethereum.

## Overview

The fork demo shows a complete workflow:
1. **Deploy** a demo coin contract to testnet
2. **Mint** tokens to a specific user address
3. **Extract** object IDs and checkpoint number
4. **Test** with fork to verify the user has the correct balance from on-chain state

## Quick Start

### Prerequisites

1. **Build Sui CLI with fork support:**
   ```bash
   cd ../../..
   cargo build --bin sui --release
   # The binary will be at target/release/sui
   ```

2. **Configure Sui client for testnet:**
   ```bash
   sui client new-env --alias testnet --rpc https://fullnode.testnet.sui.io:443
   sui client switch --env testnet
   ```

3. **Get testnet tokens:**
   - Request tokens from the Discord faucet: https://discord.com/channels/916379725201563759/971488439931392130
   - Or use: `sui client faucet`

### Step-by-Step Workflow

Navigate to the fork-demo directory:
```bash
cd examples/move/fork-demo
```

#### Step 1: Deploy the Contract

```bash
./scripts/1_deploy.sh
```

This will:
- Build the Move package
- Publish to testnet
- Save package ID and treasury cap ID to `config.json`

**Output:**
```
Package ID: 0xabcd...
Treasury Cap ID: 0x1234...
Configuration saved to config.json
```

#### Step 2: Mint Tokens and Setup Fork Data

```bash
./scripts/2_mint_and_setup.sh
```

This will:
- Mint 1,000,000 DEMO tokens to USER1 address
- Record the checkpoint number
- Extract the coin object ID
- Save object IDs to `object_ids.txt`
- Update `config.json` with test data

**Output:**
```
Minting 1,000,000 DEMO tokens to USER1...
Coin Object ID: 0x5678...
Checkpoint: 252350700
Object IDs saved to object_ids.txt
```

#### Step 3: Run Tests

```bash
./scripts/3_run_tests.sh
```

This will run tests twice:
1. **Normal mode** - without checkpoint fork (uses test scenario)
2. **Fork mode** - with checkpoint fork (uses real on-chain state)

**Expected Output:**
```
Test 1: Running tests WITHOUT checkpoint fork
BUILDING fork_demo
RUNNING Move unit tests
[ PASS    ] fork_demo::fork_tests::test_normal_mint_without_fork
[ PASS    ] fork_demo::fork_tests::test_verify_balance_from_fork
[ PASS    ] fork_demo::fork_tests::test_conditional_on_fork_state
Test result: OK. Total tests: 3; passed: 3; failed: 0

Test 2: Running tests WITH checkpoint fork
Loading checkpoint state from checkpoint 252350700...
Successfully loaded 1 objects from checkpoint 252350700
BUILDING fork_demo
RUNNING Move unit tests
[ PASS    ] fork_demo::fork_tests::test_normal_mint_without_fork
[ PASS    ] fork_demo::fork_tests::test_verify_balance_from_fork
[ PASS    ] fork_demo::fork_tests::test_conditional_on_fork_state
Test result: OK. Total tests: 3; passed: 3; failed: 0
```

## Manual Testing

You can also run tests manually with custom parameters:

### Normal Tests (No Fork)
```bash
sui move test
```

### Fork Tests with Specific Checkpoint
```bash
sui move test \
    --fork-checkpoint 252350700 \
    --fork-rpc-url https://fullnode.testnet.sui.io:443 \
    --object-id-file object_ids.txt
```

### Fork Tests with Automatic Object Scanning
```bash
# Scans last 1000 checkpoints for modified objects
sui move test \
    --fork-checkpoint 252350700 \
    --fork-rpc-url https://fullnode.testnet.sui.io:443
```

## Project Structure

```
fork-demo/
├── Move.toml                   # Package manifest
├── sources/
│   └── demo_coin.move         # Demo coin contract
├── tests/
│   └── fork_tests.move        # Test suite with fork tests
├── scripts/
│   ├── 1_deploy.sh            # Deploy contract
│   ├── 2_mint_and_setup.sh    # Mint tokens and setup fork data
│   └── 3_run_tests.sh         # Run all tests
├── config.json                # Generated: deployment config
├── object_ids.txt             # Generated: object IDs for fork
└── README.md                  # This file
```

## Test Descriptions

### `test_normal_mint_without_fork`
- Creates test state locally using `init_for_testing`
- Mints and transfers coins in test scenario
- Verifies balance
- **Works in both normal and fork mode**

### `test_verify_balance_from_fork`
- Expects USER1 to have DEMO_COIN from checkpoint state
- Verifies the balance matches MINT_AMOUNT (1,000,000)
- **Only meaningful with checkpoint fork**
- Gracefully skips if no forked state exists

### `test_conditional_on_fork_state`
- Demonstrates writing tests that adapt to fork mode
- Uses `has_most_recent_for_sender` to detect forked objects
- **Works correctly in both modes**

## Understanding Fork Testing

### Normal Mode (No Fork)
```move
let mut scenario = ts::begin(USER1);
// scenario starts with empty state
// must create all objects in test
```

### Fork Mode (With --fork-checkpoint)
```move
let mut scenario = ts::begin(USER1);
// scenario starts with objects from checkpoint
// can access real on-chain objects
if (ts::has_most_recent_for_sender<Coin<DEMO_COIN>>(&scenario)) {
    let coin = ts::take_from_sender<Coin<DEMO_COIN>>(&scenario);
    // This is a REAL coin from the blockchain!
}
```

## Configuration Files

### config.json
```json
{
  "packageId": "0xabcd...",
  "treasuryCapId": "0x1234...",
  "adminAddress": "0x...",
  "checkpoint": "252350700",
  "user1CoinId": "0x5678...",
  "user1Address": "0x1111..."
}
```

### object_ids.txt
```
# Object IDs for fork testing
# Coin owned by USER1 (0x1111...)
0x5678...
```

## Advanced Usage

### Custom USER1 Address

Edit `scripts/2_mint_and_setup.sh` and change:
```bash
USER1_ADDRESS="0x1111111111111111111111111111111111111111111111111111111111111111"
```

### Testing with Multiple Objects

Add more object IDs to `object_ids.txt`:
```
0x5678...  # First coin
0x9abc...  # Second coin
0xdef0...  # Third coin
```

### Testing Specific Test Functions

```bash
sui move test --filter test_verify_balance_from_fork \
    --fork-checkpoint 252350700 \
    --fork-rpc-url https://fullnode.testnet.sui.io:443
```

## Comparison: Normal vs Fork Testing

| Aspect | Normal Testing | Fork Testing |
|--------|---------------|--------------|
| **State** | Created in test | Loaded from checkpoint |
| **Objects** | Test objects only | Real on-chain objects |
| **Speed** | Fast (no RPC) | Slower (RPC loading) |
| **Use Case** | Unit testing logic | Integration testing with real state |
| **Reproducibility** | Always same | Depends on checkpoint |

## Troubleshooting

### "Error: config.json not found"
Run `scripts/1_deploy.sh` first to deploy the contract.

### "Error: Sui client not configured"
Configure sui client with `sui client new-env` and get testnet tokens.

### "Failed to fetch checkpoint"
- Ensure RPC URL is correct
- Check network connectivity
- Verify checkpoint number exists

### "Object not found in checkpoint"
- The object may not exist at that checkpoint
- Try a later checkpoint (after the mint transaction)
- Check object_ids.txt has correct IDs

### Tests pass without fork but fail with fork
- Verify USER1 address in test matches the address used in minting
- Check that the checkpoint is after the mint transaction
- Ensure object IDs in object_ids.txt are correct

## Benefits of Fork Testing

1. **Real State Testing**: Test against actual on-chain data
2. **Bug Reproduction**: Reproduce issues that occurred at specific checkpoints
3. **Upgrade Validation**: Test contract upgrades with production data
4. **Integration Testing**: Verify interactions with deployed contracts
5. **State Analysis**: Inspect and analyze historical blockchain states

## Next Steps

- Modify `demo_coin.move` to add more functionality
- Create more complex test scenarios in `fork_tests.move`
- Test with mainnet checkpoints (use `https://fullnode.mainnet.sui.io:443`)
- Experiment with different checkpoint numbers
- Load multiple object types for comprehensive testing

## Learn More

- [Sui Documentation](https://docs.sui.io)
- [Move Language](https://move-language.github.io/move/)
- [Sui Move Test Guide](../../../docs/content/guides/developer/first-app/build-test.mdx)
