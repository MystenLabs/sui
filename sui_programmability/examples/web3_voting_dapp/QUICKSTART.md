# Quick Start Guide - Voting DApp

This guide will get you up and running with the Voting DApp in minutes.

## Prerequisites

- Sui CLI installed ([Installation Guide](https://docs.sui.io/build/install))
- Active Sui wallet with test tokens

## Quick Setup

### 1. Deploy the Contract

```bash
cd sui_programmability/examples/web3_voting_dapp

# Make scripts executable (if not already)
chmod +x scripts/*.sh

# Deploy to current network
./scripts/deploy.sh
```

The deployment script will:
- Build the Move package
- Publish it to the blockchain
- Display the Package ID
- Save deployment info to `deployment-info.json`

### 2. Test the Contract

```bash
# Test with the deployed package
./scripts/test_contract.sh

# Or specify package ID manually
./scripts/test_contract.sh <PACKAGE_ID>
```

The test script will:
- Create a test poll
- Cast a vote
- Query poll results
- Close the poll
- Verify all operations

### 3. Manual Testing

#### Create a Poll

```bash
export PACKAGE_ID=<your_package_id>

sui client call \
  --package $PACKAGE_ID \
  --module voting \
  --function create_poll \
  --args "What's your favorite color?" "Red" "Blue" \
  --gas-budget 10000
```

Save the Poll Object ID from the output.

#### Vote on a Poll

```bash
export POLL_ID=<your_poll_id>

sui client call \
  --package $PACKAGE_ID \
  --module voting \
  --function vote \
  --args $POLL_ID 0 \
  --gas-budget 10000
```

#### View Poll Results

```bash
sui client object $POLL_ID
```

## Running Move Tests

```bash
# Run all tests
sui move test

# Run specific test
sui move test test_create_poll

# Run with verbose output
sui move test -v
```

## Project Structure

```
web3_voting_dapp/
├── Move.toml                 # Package configuration
├── sources/
│   └── voting.move          # Main contract (223 lines)
├── tests/
│   └── voting_tests.move    # Test suite (11 tests)
├── scripts/
│   ├── deploy.sh            # Deployment automation
│   └── test_contract.sh     # Contract testing
└── docs/
    ├── README.md            # Full documentation
    ├── QUICKSTART.md        # This file
    ├── DEPLOYMENT_GUIDE.md  # Detailed deployment
    └── EXAMPLES.md          # Code examples
```

## Common Operations

### Create Poll with Multiple Options

```bash
sui client call \
  --package $PACKAGE_ID \
  --module voting \
  --function create_poll_multi \
  --args "Best language?" '["Rust","Move","Python","Go"]' \
  --gas-budget 10000
```

### Close a Poll (Creator Only)

```bash
sui client call \
  --package $PACKAGE_ID \
  --module voting \
  --function close_poll \
  --args $POLL_ID \
  --gas-budget 10000
```

### Reopen a Poll (Creator Only)

```bash
sui client call \
  --package $PACKAGE_ID \
  --module voting \
  --function reopen_poll \
  --args $POLL_ID \
  --gas-budget 10000
```

## Contract Features

### Structures

- **Poll**: Main voting structure
  - `question`: Poll question (bytes)
  - `options`: List of voting options
  - `votes`: Vote counts per option
  - `total_votes`: Total number of votes
  - `creator`: Address of poll creator
  - `is_active`: Whether poll accepts votes

- **VoteReceipt**: Proof of voting (NFT)
  - `poll_id`: ID of the poll voted on
  - `voter`: Address of voter
  - `option_index`: Option that was voted for

### Entry Functions

- `create_poll(question, option1, option2, ctx)` - Create poll with 2 options
- `create_poll_multi(question, options, ctx)` - Create poll with N options
- `vote(poll, option_index, ctx)` - Cast a vote
- `close_poll(poll, ctx)` - Close poll (creator only)
- `reopen_poll(poll, ctx)` - Reopen poll (creator only)

### View Functions

- `get_question(poll)` - Get poll question
- `get_options_count(poll)` - Get number of options
- `get_option(poll, index)` - Get specific option
- `get_votes_for_option(poll, index)` - Get votes for option
- `get_total_votes(poll)` - Get total votes
- `is_active(poll)` - Check if poll is active
- `get_creator(poll)` - Get creator address

## Error Codes

- `0` - EInvalidOption: Invalid option index
- `1` - EPollNotActive: Poll is not active
- `2` - ENotCreator: Only creator can perform this action
- `3` - EInsufficientOptions: Need at least 2 options

## Network Configuration

### Switch to Devnet

```bash
sui client new-env --alias devnet --rpc https://fullnode.devnet.sui.io:443
sui client switch --env devnet
```

### Switch to Testnet

```bash
sui client new-env --alias testnet --rpc https://fullnode.testnet.sui.io:443
sui client switch --env testnet
```

### Get Test Tokens

```bash
sui client faucet
```

Or visit the [Sui Discord](https://discord.gg/sui) #devnet-faucet channel.

## Troubleshooting

### "sui: command not found"

Install Sui CLI:
```bash
cargo install --locked --git https://github.com/MystenLabs/sui.git --branch main sui
```

### "Insufficient gas"

Get more test tokens:
```bash
sui client faucet
```

### "Object not found"

Ensure you're on the correct network:
```bash
sui client active-env
```

### Build Errors

Update Sui CLI to match repository version:
```bash
cargo install --locked --git https://github.com/MystenLabs/sui.git --branch main sui --force
```

## Next Steps

1. **Explore the Code**: Read `sources/voting.move` to understand the contract
2. **Run Tests**: Execute `sui move test` to see all tests pass
3. **Deploy**: Use `./scripts/deploy.sh` to deploy your own instance
4. **Integrate**: Build a frontend or integrate with existing dApps
5. **Extend**: Add features like weighted voting, time limits, or private polls

## Support

- [Sui Documentation](https://docs.sui.io/)
- [Move Language Guide](https://move-language.github.io/move/)
- [Sui Discord Community](https://discord.gg/sui)
- [GitHub Issues](https://github.com/MystenLabs/sui/issues)

## License

Apache 2.0 - See LICENSE file for details

---

Happy voting! 🗳️
