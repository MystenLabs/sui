# Send Funds CLI Implementation Plan

## Tasks

### 1. Add `get_current_epoch` method to Client
- [x] Add method to `sui-rpc-api/src/client/mod.rs` to get current epoch from the get_epoch endpoint

### 2. Add `get_current_epoch` and `get_chain_identifier` to WalletContext
- [x] Add helper methods to `sui-sdk/src/wallet_context.rs`

### 3. Modify `dry_run_or_execute_or_serialize` for replay protection
- [x] Add TransactionExpiration::ValidDuring when gas payment is empty (stateless mode)
- [x] Generate nonce and fetch chain_id and epoch
- [x] Create new `dry_run_or_execute_or_serialize_with_address_balance_gas` function

### 4. Add `SendFunds` command variant to `SuiClientCommands`
- [x] Add struct with fields: `to`, `amount`, `coin_type`, `stateless` flag, gas args, processing args

### 5. Implement `SendFunds` command execution
- [x] For stateless: build PTB using address balance withdrawal + balance::redeem_funds + balance::send_funds
- [x] For coins: build PTB using SplitCoins + coin::send_funds
- [x] Wire up with dry_run_or_execute_or_serialize
- [x] Auto-select between coins and address balance based on availability

### 6. Add TransactionData helper
- [x] Add `new_with_gas_data_and_expiration` to support setting expiration

## Design Notes

### Move APIs
- `sui::coin::send_funds<T>(coin: Coin<T>, recipient: address)` - takes a Coin
- `sui::balance::send_funds<T>(balance: Balance<T>, recipient: address)` - takes a Balance
- `sui::balance::redeem_funds<T>(withdrawal: Withdrawal<Balance<T>>)` - redeems withdrawal to Balance

### Transaction Expiration for Replay Protection
When using address balance for gas (stateless transactions), must use:
```rust
TransactionExpiration::ValidDuring {
    min_epoch: Some(current_epoch),
    max_epoch: Some(current_epoch.saturating_add(1)),
    min_timestamp: None,
    max_timestamp: None,
    chain: chain_identifier,
    nonce: rand::random(),
}
```
