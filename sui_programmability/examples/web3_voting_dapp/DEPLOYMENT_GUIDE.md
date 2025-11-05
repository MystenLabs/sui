# Deployment Guide - Sui Voting DApp

This guide will walk you through deploying the Voting DApp to Sui blockchain.

## Prerequisites

1. **Install Sui CLI**
   ```bash
   cargo install --locked --git https://github.com/MystenLabs/sui.git --branch main sui
   ```

2. **Create Sui Wallet**
   ```bash
   sui client
   # Follow prompts to create a new wallet
   ```

3. **Get Test Tokens** (for devnet/testnet)
   - Visit [Sui Devnet Faucet](https://discordapp.com/channels/916379725201563759/971488439931392130)
   - Or use CLI: `sui client faucet`

## Step 1: Configure Network

Choose your target network:

### Devnet (Recommended for testing)
```bash
sui client new-env --alias devnet --rpc https://fullnode.devnet.sui.io:443
sui client switch --env devnet
```

### Testnet
```bash
sui client new-env --alias testnet --rpc https://fullnode.testnet.sui.io:443
sui client switch --env testnet
```

### Mainnet (Production only)
```bash
sui client new-env --alias mainnet --rpc https://fullnode.mainnet.sui.io:443
sui client switch --env mainnet
```

## Step 2: Build the Move Package

```bash
cd sui_programmability/examples/web3_voting_dapp

# Build the package
sui move build

# Expected output:
# BUILDING VotingDApp
# Successfully built Move module 'voting'
```

## Step 3: Publish the Package

```bash
# Publish to selected network
sui client publish --gas-budget 50000

# Save the output - you'll need:
# - Package ID (e.g., 0x123abc...)
# - Transaction Digest
```

Example output:
```
----- Transaction Digest ----
ABC123xyz...

----- Transaction Data ----
Transaction Signature: ...
Transaction Kind : Publish

----- Transaction Effects ----
Status : Success
Created Objects:
  - ID: 0x456def... , Owner: Immutable
    ^ This is your PACKAGE_ID
```

## Step 4: Test the Contract

### Create a Test Poll

```bash
export PACKAGE_ID=0xYOUR_PACKAGE_ID_HERE

sui client call \
  --package $PACKAGE_ID \
  --module voting \
  --function create_poll \
  --args "Which feature should we build next?" "Mobile App" "Desktop App" \
  --gas-budget 10000
```

Save the Poll Object ID from the output:
```
Created Objects:
  - ID: 0x789ghi... , Owner: Shared
    ^ This is your POLL_ID
```

### Cast a Vote

```bash
export POLL_ID=0xYOUR_POLL_ID_HERE

sui client call \
  --package $PACKAGE_ID \
  --module voting \
  --function vote \
  --args $POLL_ID 0 \
  --gas-budget 10000
```

### View Poll Data

```bash
sui client object $POLL_ID --json
```

## Step 5: Configure Frontend

1. **Update Configuration**

   Edit `frontend/app.js`:
   ```javascript
   const PACKAGE_ID = '0xYOUR_PACKAGE_ID_HERE';
   const NETWORK = 'devnet'; // or 'testnet' / 'mainnet'
   ```

2. **Install Dependencies**
   ```bash
   cd frontend
   npm install
   ```

3. **Run Development Server**
   ```bash
   npm run dev
   ```

4. **Access the DApp**
   - Open browser to `http://localhost:3000`
   - Install Sui Wallet extension if not already installed
   - Connect your wallet
   - Start creating and voting on polls!

## Step 6: Production Deployment

### Build Frontend for Production

```bash
cd frontend
npm run build
```

### Deploy to Hosting Service

#### Option 1: Vercel
```bash
npm install -g vercel
vercel --prod
```

#### Option 2: Netlify
```bash
npm install -g netlify-cli
netlify deploy --prod --dir=dist
```

#### Option 3: GitHub Pages
```bash
# Add to package.json:
"homepage": "https://yourusername.github.io/voting-dapp",
"predeploy": "npm run build",
"deploy": "gh-pages -d dist"

npm install --save-dev gh-pages
npm run deploy
```

## Verification Checklist

- [ ] Sui CLI installed and configured
- [ ] Wallet created and funded with test tokens
- [ ] Move package built successfully
- [ ] Package published to blockchain
- [ ] Package ID saved and updated in frontend
- [ ] Test poll created via CLI
- [ ] Test vote cast successfully
- [ ] Frontend dependencies installed
- [ ] Frontend app.js configured with correct Package ID
- [ ] Development server running
- [ ] Wallet extension installed in browser
- [ ] Successfully connected wallet to DApp
- [ ] Created poll through web interface
- [ ] Cast vote through web interface
- [ ] Viewed results through web interface

## Common Issues & Solutions

### Issue: "Insufficient Gas"
**Solution**: Increase gas budget or get more SUI tokens
```bash
sui client faucet
# Wait a minute, then retry
```

### Issue: "Package Not Found"
**Solution**: Verify Package ID is correct and network matches
```bash
sui client active-env
sui client objects
```

### Issue: "Object Not Found"
**Solution**: Ensure you're on the correct network and object ID is valid
```bash
sui client object $POLL_ID
```

### Issue: "Already Voted"
**Solution**: Each address can only vote once per poll. Try with a different wallet.

### Issue: Frontend Can't Connect to Wallet
**Solution**:
1. Install Sui Wallet extension
2. Unlock the wallet
3. Switch to correct network in wallet settings
4. Refresh the page

## Network Information

### Devnet
- RPC: `https://fullnode.devnet.sui.io:443`
- Explorer: `https://suiexplorer.com/?network=devnet`
- Faucet: Discord or CLI

### Testnet
- RPC: `https://fullnode.testnet.sui.io:443`
- Explorer: `https://suiexplorer.com/?network=testnet`
- Faucet: Discord

### Mainnet
- RPC: `https://fullnode.mainnet.sui.io:443`
- Explorer: `https://suiexplorer.com/`
- Note: Real SUI tokens required

## Monitoring & Analytics

### View Transaction on Explorer

```
https://suiexplorer.com/object/$POLL_ID?network=devnet
https://suiexplorer.com/txblock/$TX_DIGEST?network=devnet
```

### Query Events

```bash
sui client events --package $PACKAGE_ID
```

### Get Object Details

```bash
sui client object $POLL_ID --json | jq '.details.data.fields'
```

## Upgrading the Contract

To upgrade your deployed contract:

1. Update the Move code
2. Rebuild: `sui move build`
3. Publish upgrade: `sui client upgrade --gas-budget 50000`
4. Update Package ID in frontend

## Support Resources

- [Sui Documentation](https://docs.sui.io/)
- [Sui Discord](https://discord.gg/sui)
- [Sui GitHub](https://github.com/MystenLabs/sui)
- [Move Book](https://move-language.github.io/move/)

## Security Considerations

1. **Test Thoroughly**: Always test on devnet before mainnet
2. **Audit Code**: Consider professional audit for production
3. **Monitor Gas**: Set appropriate gas budgets
4. **Key Management**: Secure your wallet private keys
5. **Rate Limiting**: Implement rate limiting on frontend for production

---

Need help? Open an issue or join the Sui Discord community!
