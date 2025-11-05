# Sui Voting DApp - Decentralized Voting Application

A full-stack decentralized voting application built on Sui blockchain. This DApp allows users to create polls, cast votes, and view real-time results in a secure and transparent manner.

## Features

- **Create Polls**: Create voting polls with custom questions and multiple options
- **Cast Votes**: Vote on active polls using your Sui wallet
- **View Results**: See real-time voting results with visual charts
- **Poll Management**: Creators can close or reopen their polls
- **Vote Receipts**: Each vote generates a receipt NFT as proof of participation
- **Transparent**: All votes are recorded on-chain and publicly verifiable

## Project Structure

```
web3_voting_dapp/
├── Move.toml                 # Move package configuration
├── sources/
│   └── voting.move          # Smart contract implementation
├── frontend/
│   ├── index.html           # Main HTML page
│   ├── app.js               # JavaScript logic for wallet and blockchain interaction
│   ├── styles.css           # Styling
│   └── package.json         # Frontend dependencies
└── README.md                # This file
```

## Smart Contract Overview

### Structures

- **Poll**: Represents a voting poll with question, options, votes, and metadata
- **VoteReceipt**: NFT receipt issued to voters as proof of participation

### Functions

#### Entry Functions

- `create_poll(question, option1, option2, ctx)`: Create a poll with 2 options
- `create_poll_multi(question, options, ctx)`: Create a poll with multiple options
- `vote(poll, option_index, ctx)`: Cast a vote on a poll
- `close_poll(poll, ctx)`: Close a poll (creator only)
- `reopen_poll(poll, ctx)`: Reopen a closed poll (creator only)

#### View Functions

- `get_question(poll)`: Get the poll question
- `get_options(poll)`: Get all poll options
- `get_votes(poll)`: Get vote counts for each option
- `get_total_votes(poll)`: Get total number of votes
- `is_active(poll)`: Check if poll is active
- `get_creator(poll)`: Get the address of poll creator

### Events

- `PollCreated`: Emitted when a new poll is created
- `VoteCast`: Emitted when a vote is cast

## Prerequisites

### For Smart Contract Development

- Sui CLI tools installed ([Installation Guide](https://docs.sui.io/build/install))
- Rust toolchain (for building Move contracts)

### For Frontend

- Node.js v16 or higher
- npm or yarn
- Sui Wallet browser extension

## Installation & Deployment

### 1. Deploy Smart Contract

```bash
# Navigate to the voting dapp directory
cd sui_programmability/examples/web3_voting_dapp

# Build the Move package
sui move build

# Publish to Sui network (devnet/testnet/mainnet)
sui client publish --gas-budget 30000

# Save the Package ID from the output
```

### 2. Configure Frontend

```bash
# Navigate to frontend directory
cd frontend

# Install dependencies
npm install

# Update app.js with your deployed Package ID
# Replace 'YOUR_PACKAGE_ID' with the actual package ID from deployment
```

### 3. Run Frontend

```bash
# Start development server
npm run dev

# Or build for production
npm run build
npm run preview
```

## Usage Guide

### Creating a Poll

1. Connect your Sui wallet
2. Navigate to "Create Poll" tab
3. Enter your question
4. Add at least 2 options (you can add more with "+ Add Option" button)
5. Click "Create Poll"
6. Confirm the transaction in your wallet
7. Save the Poll ID from the transaction result

### Voting on a Poll

1. Connect your Sui wallet
2. Navigate to "Vote" tab
3. Enter the Poll ID
4. Click "Load Poll"
5. Select your preferred option
6. Click "Submit Vote"
7. Confirm the transaction in your wallet
8. You'll receive a VoteReceipt NFT

### Viewing Results

1. Navigate to "View Results" tab
2. Enter the Poll ID
3. Click "Load Results"
4. View the vote distribution with percentage bars
5. If you're the poll creator, you can close/reopen the poll

## Code Examples

### Creating a Poll via CLI

```bash
sui client call \
  --package $PACKAGE_ID \
  --module voting \
  --function create_poll \
  --args "What's your favorite blockchain?" "Sui" "Ethereum" \
  --gas-budget 10000
```

### Voting via CLI

```bash
sui client call \
  --package $PACKAGE_ID \
  --module voting \
  --function vote \
  --args $POLL_OBJECT_ID 0 \
  --gas-budget 10000
```

### Querying Poll Data

```bash
sui client object $POLL_OBJECT_ID
```

## Security Features

- **No Double Voting**: Each vote creates a unique receipt, preventing duplicate votes from the same address
- **Creator Controls**: Only poll creators can close or reopen their polls
- **Immutable Votes**: Once cast, votes cannot be changed or deleted
- **Transparent**: All data is stored on-chain and publicly accessible

## Development

### Running Tests

```bash
# Run Move tests
sui move test
```

### Local Development

```bash
# Start local Sui network
sui start

# In another terminal, deploy to local network
sui client publish --gas-budget 30000
```

## Troubleshooting

### Wallet Connection Issues

- Ensure Sui Wallet extension is installed and unlocked
- Switch to the correct network (devnet/testnet/mainnet)
- Refresh the page and try connecting again

### Transaction Failures

- Check you have sufficient SUI tokens for gas fees
- Verify the Poll ID is correct and exists
- Ensure the poll is still active (for voting)
- Make sure you haven't voted on this poll already

### Build Errors

- Ensure Sui CLI is up to date: `sui client --version`
- Check that all dependencies are properly installed
- Verify Move.toml configuration is correct

## Advanced Features

### Extending the Contract

You can extend the voting contract with additional features:

- **Weighted Voting**: Add token-weighted votes
- **Time-Limited Polls**: Add start/end timestamps
- **Private Polls**: Implement access control for voters
- **Delegated Voting**: Allow vote delegation to other addresses
- **Multi-Choice Voting**: Allow selecting multiple options

### Example: Adding Time Limits

```move
struct Poll has key, store {
    // ... existing fields ...
    start_time: u64,
    end_time: u64,
}

public entry fun vote(
    poll: &mut Poll,
    option_index: u64,
    clock: &Clock,
    ctx: &mut TxContext
) {
    let current_time = clock::timestamp_ms(clock);
    assert!(current_time >= poll.start_time, ENotStarted);
    assert!(current_time <= poll.end_time, EEnded);
    // ... rest of vote logic ...
}
```

## Contributing

Contributions are welcome! Please follow these guidelines:

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests for new functionality
5. Submit a pull request

## License

This project is licensed under Apache 2.0 - see the LICENSE file for details.

## Resources

- [Sui Documentation](https://docs.sui.io/)
- [Move Language Book](https://move-language.github.io/move/)
- [Sui Developer Portal](https://sui.io/developers)
- [Sui TypeScript SDK](https://github.com/MystenLabs/sui/tree/main/sdk/typescript)

## Support

For issues and questions:
- Open an issue on GitHub
- Join Sui Discord community
- Check Sui documentation

## Roadmap

- [ ] Add support for ranked-choice voting
- [ ] Implement vote delegation
- [ ] Add poll templates
- [ ] Create mobile app version
- [ ] Add analytics dashboard
- [ ] Support for multi-sig poll creation

---

Built with ❤️ on Sui Blockchain
