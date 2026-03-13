# Sui-Rosetta: Deep Codebase Research

> **Branch**: `main` (master baseline)
> **Date**: 2026-03-12
> **Mode**: Combined Capability Assessment + Flow Trace
> **Note**: This document describes the master branch state. Feature branch additions (FungibleStake, AllStakes) are noted separately where relevant.

---

## Table of Contents

1. [What is Rosetta (Mesh API)?](#1-what-is-rosetta-mesh-api)
2. [Service Identity](#2-service-identity)
3. [Architecture Overview](#3-architecture-overview)
4. [Core Concepts & Vocabulary](#4-core-concepts--vocabulary)
5. [Supported Operations](#5-supported-operations)
6. [Data API Flow Traces](#6-data-api-flow-traces)
7. [Construction API Flow Trace](#7-construction-api-flow-trace)
8. [Internal Operations & Transaction Building](#8-internal-operations--transaction-building)
9. [Gas Handling: Coin Gas vs Address-Balance Gas](#9-gas-handling-coin-gas-vs-address-balance-gas)
10. [Staking Operations Deep Dive](#10-staking-operations-deep-dive)
11. [Balance Query & Sub-Accounts](#11-balance-query--sub-accounts)
12. [Operation Parsing: The Round-Trip Problem](#12-operation-parsing-the-round-trip-problem)
13. [Current Status & Recent Work](#13-current-status--recent-work)
14. [Capability Matrix](#14-capability-matrix)

---

## 1. What is Rosetta (Mesh API)?

**Rosetta** (now rebranded as **Mesh API** by Coinbase) is an open-source specification for blockchain integration. Its goal is to make blockchain integration **simpler, faster, and more reliable** than writing native integrations for each chain.

### The Problem It Solves

Every blockchain has a unique API, data model, and transaction format. An exchange like Coinbase that supports hundreds of chains would need hundreds of bespoke integrations. Rosetta standardizes this into a single interface:

- **One API spec** that every chain implements
- **Standard operation types** (transfers, staking, etc.) expressed as balance-changing operations
- **Standard construction flow** for building, signing, and submitting transactions
- **Automated testing** via `rosetta-cli` that validates correctness

### Two API Groups

| API Group | Purpose | Server Type |
|-----------|---------|-------------|
| **Data API** | Read blockchain data: balances, blocks, transactions | Online (needs full node) |
| **Construction API** | Build, sign, submit transactions | Split: Offline (no network) + Online (metadata, submit) |

The Construction API is deliberately split so that **private key operations happen on an air-gapped offline server**, while only metadata fetching and submission require network access.

### The Key Abstraction: Operations

Rosetta models everything as **Operations** — atomic balance-changing actions. A simple SUI transfer becomes:

```
Operation 0: sender  -100 SUI  (PaySui)
Operation 1: receiver +100 SUI (PaySui)
```

A stake becomes:

```
Operation 0: sender -1000 SUI (Stake, metadata: {validator: 0x...})
```

The contract: **intent operations** (what you want to do) must be a prefix of **confirmed operations** (what actually happened on-chain). Confirmed operations may include additional operations like gas charges and balance changes.

---

## 2. Service Identity

**Sui-Rosetta** is the Mesh API (f.k.a. Rosetta API) implementation for the Sui network. It translates between the Rosetta standard and Sui's object-centric, programmable transaction model.

It exists because Sui's model is fundamentally different from account-based chains:
- Sui has **objects** (coins, stakes) not account balances (though address-balance is now supported)
- Sui uses **programmable transaction blocks** (PTBs) not simple transfer instructions
- Sui has **checkpoints** not traditional blocks

Sui-Rosetta bridges this gap, presenting Sui's unique model through Rosetta's standardized lens.

**Source**: `src/lib.rs:31` — *"This lib implements the Mesh online and offline server defined by the Mesh API Spec"*

---

## 3. Architecture Overview

### Two Servers, One Codebase

```
┌─────────────────────────────────────────────────────────────────┐
│                     Rosetta Client (e.g. Coinbase)              │
│                                                                 │
│  Construction Flow:                   Data Flow:                │
│  preprocess → metadata → payloads     /block                    │
│  → [sign] → combine → submit         /block/transaction        │
│                                       /account/balance          │
└────────┬──────────────┬───────────────┬─────────────────────────┘
         │              │               │
    ┌────▼────┐    ┌────▼────┐    ┌─────▼─────┐
    │ Offline │    │ Online  │    │  Online   │
    │ Server  │    │ Server  │    │  Server   │
    │ :9003   │    │ :9002   │    │  :9002    │
    └─────────┘    └────┬────┘    └─────┬─────┘
                        │               │
                   ┌────▼───────────────▼────┐
                   │  Sui Full Node (gRPC)   │
                   │  sui_rpc::client::Client│
                   └─────────────────────────┘
```

**Source**: `src/lib.rs:60-133`

### Offline Server Routes (`:9003`)

Handles operations that **don't need network access** — all cryptographic and data transformation work:

| Route | Handler | Purpose |
|-------|---------|---------|
| `/construction/derive` | `construction::derive` | Public key → SuiAddress |
| `/construction/payloads` | `construction::payloads` | Operations + metadata → unsigned tx + signing payloads |
| `/construction/combine` | `construction::combine` | Unsigned tx + signatures → signed tx |
| `/construction/preprocess` | `construction::preprocess` | Operations → metadata request options |
| `/construction/hash` | `construction::hash` | Signed tx → transaction hash |
| `/construction/parse` | `construction::parse` | Tx bytes → operations (round-trip verification) |
| `/network/list` | `network::list` | Available networks |
| `/network/options` | `network::options` | Supported operation types |

### Online Server Routes (`:9002`)

Handles operations that **need the full node**:

| Route | Handler | Purpose |
|-------|---------|---------|
| `/account/balance` | `account::balance` | Get account balances (SUI, custom coins, staking sub-accounts) |
| `/account/coins` | `account::coins` | Get unspent coin objects |
| `/block` | `block::block` | Get a Sui checkpoint as a Rosetta block |
| `/block/transaction` | `block::transaction` | Get a specific transaction's operations |
| `/construction/submit` | `construction::submit` | Simulate + execute a signed transaction |
| `/construction/metadata` | `construction::metadata` | Fetch object refs, gas estimation, gas price |
| `/network/status` | `network::status` | Current block, sync status, peers/validators |
| `/network/list` | `network::list` | Available networks |
| `/network/options` | `network::options` | Supported operation types |

### Key Internal Types

| Type | Location | Purpose |
|------|----------|---------|
| `OnlineServerContext` | `state.rs:28` | Holds gRPC client, block provider, coin cache, chain ID |
| `CheckpointBlockProvider` | `state.rs:70` | Maps Sui checkpoints to Rosetta blocks |
| `CoinMetadataCache` | `lib.rs:136` | LRU cache (1000 entries) for coin type → Currency lookups |
| `Operations` | `operations.rs:43` | Ordered list of `Operation` — the core Rosetta abstraction |
| `InternalOperation` | `types/internal_operation.rs:80` | Enum of supported tx types: PaySui, PayCoin, Stake, WithdrawStake |

---

## 4. Core Concepts & Vocabulary

### Operation

The fundamental Rosetta unit. Every balance change is an operation.

```rust
// operations.rs:43
pub struct Operations(Vec<Operation>);

// Each Operation (defined later in operations.rs):
struct Operation {
    operation_identifier: OperationIdentifier,  // sequential index
    type_: OperationType,                       // PaySui, Stake, Gas, etc.
    status: Option<OperationStatus>,            // Success/Failure (None for intents)
    account: Option<AccountIdentifier>,         // who is affected
    amount: Option<Amount>,                     // how much (negative = sender)
    coin_change: Option<CoinChange>,            // UTXO-style coin tracking
    metadata: Option<OperationMetadata>,        // extra data (validator for Stake)
}
```

### OperationType

All possible operation types in sui-rosetta:

```rust
// types.rs:433-454
pub enum OperationType {
    // Balance-changing from TransactionEffect:
    Gas,                    // gas charges
    SuiBalanceChange,       // net balance change from tx
    StakeReward,            // reward portion of unstake
    StakePrinciple,         // principal portion of unstake

    // Supported construction types:
    PaySui,                 // transfer SUI
    PayCoin,                // transfer non-SUI coins
    Stake,                  // delegate SUI to validator
    WithdrawStake,          // undelegate/unstake

    // Read-only system types:
    EpochChange, Genesis, ConsensusCommitPrologue,
    ProgrammableTransaction, AuthenticatorStateUpdate,
    RandomnessStateUpdate, EndOfEpochTransaction,
    ProgrammableSystemTransaction, Unknown,
}
```

### InternalOperation

The bridge between Rosetta operations and Sui transactions:

```rust
// types/internal_operation.rs:80-85
pub enum InternalOperation {
    PaySui(PaySui),           // {sender, recipients, amounts}
    PayCoin(PayCoin),         // {sender, recipients, amounts, currency}
    Stake(Stake),             // {sender, validator, amount}
    WithdrawStake(WithdrawStake), // {sender, stake_ids}
}
```

### SubAccountType

Balance queries support sub-accounts for staking:

```rust
// types.rs:84-88 (master)
pub enum SubAccountType {
    Stake,            // Active StakedSui objects (principal)
    PendingStake,     // StakedSui not yet activated (current_epoch < activation_epoch)
    EstimatedReward,  // Estimated rewards on active stakes
}
```

> **Feature branch adds**: `FungibleStake` (query `FungibleStakedSui` objects) and `AllStakes` (combined StakedSui + FungibleStakedSui).
```

### Block ≡ Checkpoint

Rosetta's "block" maps to Sui's "checkpoint":

```rust
// types.rs:33
pub type BlockHeight = u64;  // checkpoint sequence number
pub type BlockHash = CheckpointDigest;
```

**Source**: `state.rs:229-291` — `create_block_response` converts a Sui Checkpoint into a Rosetta Block containing all transactions with their operations.

### Currency

```rust
// types.rs:102-107
pub struct Currency {
    pub symbol: String,      // "SUI"
    pub decimals: u64,       // 9
    pub metadata: CurrencyMetadata { coin_type: String }, // "0x2::sui::SUI"
}
```

The default SUI currency is defined as a global lazy static:
```rust
// lib.rs:52-58
pub static SUI: Lazy<Currency> = Lazy::new(|| Currency {
    symbol: "SUI".to_string(),
    decimals: 9,
    metadata: CurrencyMetadata {
        coin_type: SDKTypeTag::from(StructTag::sui()).to_string(),
    },
});
```

---

## 5. Supported Operations

### Constructable (can build + submit transactions)

| Operation | Description | What it does on-chain |
|-----------|-------------|----------------------|
| **PaySui** | Transfer SUI | Merge coins → SplitCoins → TransferObjects |
| **PayCoin** | Transfer non-SUI tokens | Merge coins → SplitCoins → TransferObjects (with type param) |
| **Stake** | Delegate SUI to validator | Merge coins → SplitCoins → `sui_system::request_add_stake` |
| **WithdrawStake** | Unstake/undelegate | `sui_system::request_withdraw_stake` for each stake object |

### Read-only (returned from Data API for executed transactions)

| Operation | Description |
|-----------|-------------|
| **Gas** | Gas charges (storage cost + computation - rebate) |
| **SuiBalanceChange** | Net balance change per address per currency |
| **StakePrinciple** | Principal returned from unstaking (from `UnstakeRequest` event) |
| **StakeReward** | Reward returned from unstaking (from `UnstakeRequest` event) |
| **EpochChange**, **Genesis**, etc. | System transaction markers |

---

## 6. Data API Flow Traces

### 6.1 `/block` — Get a Block (Checkpoint)

```
Client POST /block {block_identifier: {index: 12345}}
  → block::block()                                    [block.rs:23]
    → env.check_network_identifier()                  [types.rs:54]
    → state.blocks().get_block_by_index(12345)        [state.rs:77]
      → gRPC: GetCheckpointRequest(sequence_number=12345)
        with fields: sequence_number, digest, summary.*, transactions.*
      → create_block_response(checkpoint)             [state.rs:229]
        → For each transaction in checkpoint (buffered, max 10 concurrent):
          → Operations::try_from_executed_transaction() [operations.rs:946]
            → Parse tx kind → operations (PaySui, Stake, etc.)
            → Process balance_changes from effects
            → Process unstake events (StakePrinciple, StakeReward)
            → Handle GasCoin transfer edge cases
            → Combine: intent ops + balance change ops + gas ops + staking ops
        → Build BlockResponse with parent_block_identifier
  ← BlockResponse { block: { block_identifier, parent, timestamp, transactions } }
```

**Key detail**: Sui checkpoints can contain thousands of transactions. The code processes them with `buffer_unordered(10)` concurrency limit to prevent resource starvation (`state.rs:267`).

### 6.2 `/account/balance` — Get Account Balance

```
Client POST /account/balance {
  account_identifier: {address: "0x...", sub_account: {address: "Stake"}},
  currencies: [{symbol: "SUI", decimals: 9}]
}
  → account::balance()                                [account.rs:31]
    → get_checkpoint() — latest checkpoint for block_identifier
    → get_balances()                                   [account.rs:63]
      → If sub_account present:
          get_sub_account_balances(account_type, client, address)  [account.rs:116]
            → Stake:          list_delegated_stake, filter epoch >= activation, return principals
            → PendingStake:   list_delegated_stake, filter epoch < activation, return principals
            → EstimatedReward: list_delegated_stake, filter active, return rewards
      → Else: get_account_balances per currency via gRPC GetBalanceRequest
  ← AccountBalanceResponse { block_identifier, balances: [Amount] }
```

### 6.3 `/block/transaction` — Get Single Transaction

```
Client POST /block/transaction { transaction_identifier: {hash: "..."} }
  → block::transaction()                              [block.rs:42]
    → gRPC: GetTransactionRequest(digest)
      with fields: digest, transaction.*, effects.*, balance_changes, events.*
    → Operations::try_from_executed_transaction()     [operations.rs:946]
    ← BlockTransactionResponse { transaction: { identifier, operations } }
```

---

## 7. Construction API Flow Trace

The Construction API follows a strict 7-step flow. This is the canonical Rosetta transaction lifecycle:

```
┌──────────────────────────────────────────────────────────────────┐
│                   CONSTRUCTION FLOW                              │
│                                                                  │
│  Step 1: /construction/preprocess  (Offline)                     │
│    Input:  Operations (what you want to do)                      │
│    Output: MetadataOptions (what info is needed from network)    │
│                                                                  │
│  Step 2: /construction/metadata    (Online)                      │
│    Input:  MetadataOptions                                       │
│    Output: ConstructionMetadata (gas coins, budget, gas price)   │
│                                                                  │
│  Step 3: /construction/payloads    (Offline)                     │
│    Input:  Operations + ConstructionMetadata                     │
│    Output: Unsigned transaction + SigningPayloads                │
│                                                                  │
│  Step 4: [CLIENT SIGNS]            (Client-side)                 │
│    Signs the payload with private key                            │
│                                                                  │
│  Step 5: /construction/combine     (Offline)                     │
│    Input:  Unsigned transaction + Signatures                     │
│    Output: Signed transaction                                    │
│                                                                  │
│  Step 6: /construction/parse       (Offline)                     │
│    Input:  Signed transaction                                    │
│    Output: Operations (round-trip verification)                  │
│                                                                  │
│  Step 7: /construction/submit      (Online)                      │
│    Input:  Signed transaction                                    │
│    Output: Transaction hash                                      │
└──────────────────────────────────────────────────────────────────┘
```

### Step 1: Preprocess

```rust
// construction.rs:223-239
pub async fn preprocess(request) -> ConstructionPreprocessResponse {
    let internal_operation = request.operations.into_internal()?;
    // Converts Rosetta operations → InternalOperation enum
    let sender = internal_operation.sender();
    let budget = request.metadata.and_then(|m| m.budget);
    Ok(ConstructionPreprocessResponse {
        options: Some(MetadataOptions { internal_operation, budget }),
        required_public_keys: vec![sender.into()],
    })
}
```

The `into_internal()` call (`operations.rs:98-109`) dispatches based on operation type:
- `PaySui` → extracts sender (negative amount), recipients (positive amounts)
- `PayCoin` → same but also extracts currency
- `Stake` → extracts sender, validator (from metadata), amount
- `WithdrawStake` → extracts sender, stake_ids (from metadata)

### Step 2: Metadata (Online)

This is the most complex step — it fetches all objects needed for the transaction and estimates gas.

```rust
// construction.rs:263-336
pub async fn metadata(request) -> ConstructionMetadataResponse {
    let sender = option.internal_operation.sender();
    let gas_price = client.get_reference_gas_price() + 100; // buffer for epoch changes

    // THE KEY CALL: fetch objects + estimate gas
    let TransactionObjectData { gas_coins, objects, party_objects, total_sui_balance, budget, .. }
        = option.internal_operation
            .try_fetch_needed_objects(&mut client, Some(gas_price), budget)
            .await?;

    // Returns everything needed to build the transaction offline
    Ok(ConstructionMetadataResponse {
        metadata: ConstructionMetadata {
            sender, gas_coins, objects, party_objects,
            total_coin_value, gas_price, budget, currency, ...
        },
        suggested_fee: vec![Amount::new(budget as i128, None)],
    })
}
```

Each `InternalOperation` implements `try_fetch_needed_objects()` differently — see [Section 8](#8-internal-operations--transaction-building).

### Step 3: Payloads (Offline)

```rust
// construction.rs:63-90
pub async fn payloads(request) -> ConstructionPayloadsResponse {
    let metadata = request.metadata;  // from Step 2
    let data = request.operations
        .into_internal()?
        .try_into_data(metadata)?;   // Build Sui TransactionData

    let intent_msg = IntentMessage::new(Intent::sui_transaction(), data);
    let digest = hash(bcs::to_bytes(&intent_msg));

    Ok(ConstructionPayloadsResponse {
        unsigned_transaction: Hex(bcs(intent_msg)),
        payloads: vec![SigningPayload {
            account_identifier: sender,
            hex_bytes: Hex(digest),  // this is what gets signed
            signature_type: Ed25519,
        }],
    })
}
```

### Step 5: Combine (Offline)

```rust
// construction.rs:96-139
pub async fn combine(request) -> ConstructionCombineResponse {
    let intent_msg = bcs::from_bytes(&request.unsigned_transaction);
    let sig = request.signatures[0];

    // Reconstruct Sui signature: flag_byte + sig_bytes + public_key
    let flag = match sig.signature_type { Ed25519 => ED25519.flag(), Ecdsa => Secp256k1.flag() };
    let signed_tx = Transaction::from_generic_sig_data(
        intent_msg.value,
        vec![GenericSignature::from_bytes(&[flag, sig_bytes, pub_key].concat())?],
    );

    // Verify signature before returning
    verify_sender_signed_data_message_signatures(&signed_tx, ...)?;

    Ok(ConstructionCombineResponse {
        signed_transaction: Hex(bcs(signed_tx)),
    })
}
```

### Step 7: Submit (Online)

```rust
// construction.rs:144-217
pub async fn submit(request) -> TransactionIdentifierResponse {
    let signed_tx = bcs::from_bytes(&request.signed_transaction);

    // 1. Dry-run simulation first
    let sim_response = client.simulate_transaction(SimulateTransactionRequest {
        transaction, checks: Enabled, do_gas_selection: false,
    });
    if !sim_response.effects.status.success() {
        return Err(TransactionDryRunError);
    }

    // 2. Actually execute
    let exec_response = client.execute_transaction(ExecuteTransactionRequest {
        transaction, signatures, read_mask: "*",
    });
    if !exec_response.effects.status.success() {
        return Err(TransactionExecutionError);
    }

    Ok(TransactionIdentifierResponse { hash: digest })
}
```

**Key detail**: Submit does a simulation BEFORE execution. This catches errors before they hit the chain. The simulation uses `TransactionChecks::Enabled` and `do_gas_selection: false` (gas was already selected in metadata step).

---

## 8. Internal Operations & Transaction Building

Each `InternalOperation` variant implements two key traits:

1. **`TryConstructTransaction::try_fetch_needed_objects()`** — Called during `/construction/metadata`. Fetches coins, simulates transaction, estimates gas.
2. **`InternalOperation::try_into_data()`** — Called during `/construction/payloads`. Builds the actual `TransactionData` (the PTB) using metadata from step 1.

### 8.1 PaySui

**File**: `types/internal_operation/pay_sui.rs`

**Metadata fetch** (`try_fetch_needed_objects`):
1. Query `GetBalanceRequest` for address balance (the new address-balance feature)
2. Select up to 1500 largest `Coin<SUI>` objects
3. Separate party objects (`ConsensusAddressOwner`) from regular objects
4. Try **Path A** (address-balance gas): simulate with no gas coins → if simulator returns empty gas objects, address-balance gas is available
5. If Path A fails, fall back to **Path B** (coin gas): use first 255 non-party coins as gas, merge rest

**Transaction building** (`try_into_data` → `pay_sui_pt_ab_gas` or `pay_sui_pt_coin_gas`):

Path A (AB gas):
```
MergeCoins(first_coin, [remaining_coins...])  // merge all into one
MergeCoins(target, [party_coins...])          // merge party coins too
withdraw_coin_from_address_balance(deficit)   // if coins < payment
MergeCoins(target, [withdrawal_coin])
SplitCoins(target, [amount1, amount2, ...])   // split payment amounts
TransferObjects([split[0]], recipient1)
TransferObjects([split[1]], recipient2)
// If source was from AB, explicit TransferObjects back to sender
```

Path B (Coin gas):
```
MergeCoins(GasCoin, [extra_coins...])         // merge into gas coin
MergeCoins(GasCoin, [party_coins...])
withdraw_coin_from_address_balance(AB)        // withdraw all AB
MergeCoins(GasCoin, [withdrawn])
PaySui(recipients, amounts)                   // built-in PTB command
```

### 8.2 PayCoin

**File**: `types/internal_operation/pay_coin.rs`

Same flow as PaySui but for non-SUI coin types. Key difference:

- Coins of the specified type are used for payment, NOT as gas
- Gas always comes from SUI (either coin objects or address balance)
- A **currency marker** is appended as a pure input at the end of the PTB for round-trip parsing:

```rust
// pay_coin.rs:270-272
let currency_string = serde_json::to_string(currency)?;
builder.pure(currency_string)?;  // Not used in execution, only for /parse
```

### 8.3 Stake

**File**: `types/internal_operation/stake.rs`

**Metadata fetch**:
1. Same coin selection as PaySui (up to 1500 coins)
2. For `stake_all` (amount=None): simulate with 1 SUI placeholder, compute actual deficit later
3. Path A/B gas selection same as PaySui

**Transaction building** (`stake_pt_ab_gas` or `stake_pt_coin_gas`):

```
MergeCoins(...)                               // merge all coins
withdraw_coin_from_address_balance(deficit)    // if needed
SplitCoins(source, [amount])                  // split stake amount
MoveCall(sui_system::request_add_stake,       // stake it
         [system_state, split_coin, validator])
```

**Important workaround** (`stake.rs:288-296`): The input ordering of `validator` vs `system_state` is used as a hack to signal `stake_all` vs specific amount during round-trip parsing:
- Specific amount: `validator` input BEFORE `system_state` (validator index < system_state index)
- Stake all: `system_state` input BEFORE `validator` (system_state index < validator index)

### 8.4 WithdrawStake

**File**: `types/internal_operation/withdraw_stake.rs`

**Metadata fetch**:
1. If `stake_ids` is empty (withdraw all): list all `StakedSui` objects owned by sender
2. Fetch object refs for each stake
3. Simulate to estimate gas

**Transaction building** (`withdraw_stake_pt`):

```
For each stake_id:
    MoveCall(sui_system::request_withdraw_stake,
             [system_state, stake_object])
```

Same input ordering hack as Stake for `withdraw_all` detection.

---

## 9. Gas Handling: Coin Gas vs Address-Balance Gas

Sui-Rosetta supports two gas payment methods, tried in order:

### Path A: Address-Balance Gas (preferred)

When the Sui full node supports address-balance gas, transactions don't need explicit gas coin objects. The gas is deducted directly from the sender's address balance.

**Detection**: Simulate with empty gas coins. If the simulator returns empty `gas_objects`, address-balance gas is available.

```rust
// internal_operation.rs:199-228
if metadata.gas_coins.is_empty() {
    Ok(TransactionData::new_programmable_with_address_balance_gas(
        sender, pt, budget, gas_price, chain_id, epoch, nonce,
    ))
}
```

When using AB gas, the metadata includes:
- `epoch`: needed for `ValidDuring` expiration
- `chain_id`: genesis checkpoint digest identifying the chain
- `address_balance_withdrawal`: amount to withdraw from address balance for the payment itself (separate from gas)

### Path B: Coin Gas (fallback)

Traditional gas payment using `Coin<SUI>` objects.

```rust
// internal_operation.rs:221-227
Ok(TransactionData::new_programmable(
    sender, metadata.gas_coins, pt, budget, gas_price,
))
```

### Address-Balance Withdrawal for Payment

Even with coin gas, the payment itself may need address-balance funds if coins are insufficient. This is done via `FundsWithdrawal`:

```rust
// internal_operation.rs:234-252
pub fn withdraw_coin_from_address_balance(builder, amount, type_tag) -> Argument {
    let withdrawal_arg = builder.input(CallArg::FundsWithdrawal(
        FundsWithdrawalArg::balance_from_sender(amount, type_tag),
    ));
    let coin = builder.command(Command::move_call(
        SUI_FRAMEWORK, "coin", "redeem_funds", [type_tag], [withdrawal_arg]
    ));
    coin  // Returns a Coin<T> argument usable in the PTB
}
```

### Party Objects (ConsensusAddressOwner)

Coins owned by `ConsensusAddress` (shared ownership) cannot be used as gas but CAN be merged into other coins:

```rust
// pay_sui.rs:83-100
let (party_objects, non_party_objects): (Vec<_>, Vec<_>) = all_coins
    .iter()
    .partition(|obj| obj.owner().kind() == OwnerKind::ConsensusAddress);
```

---

## 10. Staking Operations Deep Dive

### Stake (Delegate)

**Rosetta Input**:
```json
{
  "operations": [{
    "type": "Stake",
    "account": { "address": "0xsender..." },
    "amount": { "value": "-1000000000" },
    "metadata": { "validator": "0xvalidator..." }
  }]
}
```

- Negative amount = SUI leaving the sender's balance
- `amount: null` = stake all available SUI (minus gas)
- The on-chain call is `sui_system::request_add_stake(system_state, coin, validator)`

### WithdrawStake (Undelegate)

**Rosetta Input**:
```json
{
  "operations": [{
    "type": "WithdrawStake",
    "account": { "address": "0xsender..." },
    "metadata": { "stake_ids": ["0xstake1...", "0xstake2..."] }
  }]
}
```

- `stake_ids: []` = withdraw ALL stakes
- Each stake ID results in a separate `request_withdraw_stake` call in the PTB
- On-chain: `sui_system::request_withdraw_stake(system_state, staked_sui_object)`

### Unstake Event Processing

When reading executed transactions, unstake events are parsed to separate principal from rewards:

```rust
// operations.rs:1016-1046
for event in events {
    if is_unstake_event(&type_tag) {
        // Extract from event JSON:
        principal_amounts += event.json["principal_amount"];
        reward_amounts += event.json["reward_amount"];
    }
}
// Create separate operations:
Operation::stake_principle(status, sender, principal_amounts)
Operation::stake_reward(status, sender, reward_amounts)
```

---

## 11. Balance Query & Sub-Accounts

### Standard Balance Query

Without sub-accounts, balance is fetched via gRPC `GetBalanceRequest`:

```rust
// account.rs:104-114
async fn get_account_balances(client, address, coin_type) -> i128 {
    let request = GetBalanceRequest::default()
        .with_owner(address).with_coin_type(coin_type);
    client.state_client().get_balance(request).balance() as i128
}
```

### Sub-Account: Stake

Returns active (non-pending) `StakedSui` objects as `SubBalance` entries:

```rust
// account.rs:126-134
SubAccountType::Stake => delegated_stakes
    .filter(|stake| current_epoch >= stake.activation_epoch)
    .map(|stake| SubBalance {
        stake_id: stake.staked_sui_id,
        validator: stake.validator_address,
        value: stake.principal as i128,
    })
```

### Sub-Account: FungibleStake (Feature Branch Only)

> This does NOT exist on master. Added in the `rosetta/fungible-stake-balance-query` branch.

Queries `FungibleStakedSui` objects owned by the address via `ListOwnedObjectsRequest` with type filter `0x3::staking_pool::FungibleStakedSui`. Deserializes BCS to extract `pool_id` and `value`. Returns `SubBalance` entries where `validator` is the staking pool ID (not the validator address).

### Sub-Account: AllStakes (Feature Branch Only)

> This does NOT exist on master. Added in the `rosetta/fungible-stake-balance-query` branch.

Combines active `StakedSui` (from `list_delegated_stake`) with `FungibleStakedSui` (from the above query) into a single response.

---

## 12. Operation Parsing: The Round-Trip Problem

The most complex part of sui-rosetta is **parsing executed transactions back into operations**. This must produce operations that are compatible with what the Construction API outputs.

### The Round-Trip Contract

```
Intent operations (from /construction/parse)
  MUST be a prefix of
Confirmed operations (from /block/transaction)
```

### How Parsing Works

`Operations::try_from_executed_transaction()` (`operations.rs:946-1088`) does:

1. **Parse transaction kind** → intent operations (PaySui, Stake, etc.)
2. **Process balance changes** from effects → SuiBalanceChange operations
3. **Process unstake events** → StakePrinciple + StakeReward operations
4. **Handle GasCoin transfers** → adjust gas attribution
5. **Deduplicate** → remove mutually cancelling operations (PayCoin workaround)
6. **Combine** → intent ops + balance change ops + gas ops + staking ops

### parse_programmable_transaction

The core parsing logic (`operations.rs:273-656`) walks through PTB commands and tracks values:

```rust
enum KnownValue { GasCoin(u64) }
// For each command:
//   SplitCoins  → track split amounts as KnownValue
//   TransferObjects → aggregate recipient amounts
//   MoveCall(stake) → create Stake operation
//   MoveCall(unstake) → collect stake_ids
//   MoveCall(redeem_funds) → passthrough KnownValue
//   MoveCall(send_funds) → track as transfer
//   MergeCoins → skip (doesn't affect operations)
```

### PayCoin Round-Trip Hack

PayCoin transactions include the currency as a trailing pure input:
```rust
// operations.rs:604-613
currency = inputs.iter().last().and_then(|input| {
    if input.kind() == InputKind::Pure {
        serde_json::from_str::<Currency>(&bcs::from_bytes::<String>(input.pure()))
    }
});
```

This is necessary because the PTB itself doesn't carry currency metadata — it only has type tags which aren't enough to reconstruct the Rosetta Currency object.

---

## 13. Current Status & Recent Work

### What Works Today

| Feature | Status | Notes |
|---------|--------|-------|
| PaySui (transfer SUI) | ✅ Full | AB gas + coin gas paths |
| PayCoin (transfer custom tokens) | ✅ Full | AB gas, with currency round-trip hack |
| Stake (delegate) | ✅ Full | AB gas + coin gas, stake_all support |
| WithdrawStake (undelegate) | ✅ Full | Single + all stakes |
| Balance query (SUI) | ✅ Full | Via GetBalanceRequest |
| Balance query (custom coins) | ✅ Full | Multi-currency support |
| Sub-account: Stake | ✅ Full | Active StakedSui principals |
| Sub-account: PendingStake | ✅ Full | Pre-activation stakes |
| Sub-account: EstimatedReward | ✅ Full | Estimated rewards |
| Sub-account: FungibleStake | ❌ Not on master | Feature branch only |
| Sub-account: AllStakes | ❌ Not on master | Feature branch only |
| Block/Transaction reading | ✅ Full | Checkpoint-based |
| Network status | ✅ Full | Validators as peers |
| Party objects (ConsensusAddress) | ✅ Full | Merged but not used as gas |
| Address-balance gas | ✅ Full | Preferred path with fallback |

### In-Progress Work (Feature Branch `rosetta/fungible-stake-balance-query`)

The following are NOT on master but exist on the feature branch:

1. **`SubAccountType::FungibleStake`** — Balance query for `FungibleStakedSui` objects
2. **`SubAccountType::AllStakes`** — Combined balance query for StakedSui + FungibleStakedSui

These are **read-only** balance queries. No construction support for fungible staking operations exists yet.

### What's NOT Supported

| Feature | Status | Reason |
|---------|--------|--------|
| `/call` (network-specific RPC) | ❌ | Not implemented |
| `/events/blocks` (block events) | ❌ | Requires indexer |
| `/mempool` | ❌ | Sui doesn't have a traditional mempool |
| `/search/transactions` | ❌ | Requires indexer |
| Fungible staking construction (stake/unstake via fungible) | ❌ | Only balance query exists; no construction support yet |
| Multi-sig transactions | ⚠️ Limited | Single signature assumed in combine |
| ZkLogin | ⚠️ Limited | Placeholder epoch=0 in combine, will likely fail |

---

## 14. Capability Matrix

### For Fungible Staking Operations (Context: Recent Work)

| Layer | FungibleStake Balance Query | FungibleStake Construction |
|-------|-----------------------------|---------------------------|
| **Sub-account type** | ✅ `SubAccountType::FungibleStake` | N/A |
| **Balance fetching** | ✅ `get_fungible_stake_balances()` | N/A |
| **BCS parsing** | ✅ `FungibleStakedSuiContent` | N/A |
| **Combined query** | ✅ `AllStakes` combines both | N/A |
| **InternalOperation** | N/A | ❌ No FungibleStake/FungibleUnstake variant |
| **OperationType** | N/A | ❌ No FungibleStake operation type |
| **PTB building** | N/A | ❌ No fungible stake/unstake transaction builder |
| **Round-trip parsing** | N/A | ❌ No parse support for fungible operations |

### For All Operations

| Capability | PaySui | PayCoin | Stake | WithdrawStake |
|------------|--------|---------|-------|---------------|
| InternalOperation variant | ✅ | ✅ | ✅ | ✅ |
| try_fetch_needed_objects | ✅ | ✅ | ✅ | ✅ |
| try_into_data (PTB build) | ✅ | ✅ | ✅ | ✅ |
| AB gas path | ✅ | ✅ | ✅ | ✅ (auto) |
| Coin gas path | ✅ | N/A | ✅ | ✅ |
| Party object merge | ✅ | ✅ | ✅ | ❌ |
| Round-trip parse | ✅ | ✅ | ✅ | ✅ |
| Balance change tracking | ✅ | ✅ | ✅ | ✅ |
| Event extraction | N/A | N/A | N/A | ✅ (principal + reward) |

---

## Appendix A: File Map

| File | Lines | Purpose |
|------|-------|---------|
| `src/lib.rs` | 189 | Server setup, routing, CoinMetadataCache |
| `src/main.rs` | 257 | CLI entry point, commands |
| `src/types.rs` | 1114 | All Rosetta type definitions |
| `src/construction.rs` | 376 | Construction API handlers |
| `src/account.rs` | 373 | Account API handlers |
| `src/block.rs` | 90 | Block API handlers |
| `src/state.rs` | 316 | OnlineServerContext, CheckpointBlockProvider |
| `src/network.rs` | 144 | Network API handlers |
| `src/errors.rs` | 147 | Error types |
| `src/operations.rs` | ~1100 | Operation parsing, round-trip logic |
| `src/types/internal_operation.rs` | 370 | InternalOperation, try_into_data |
| `src/types/internal_operation/pay_sui.rs` | 345 | PaySui PTB building |
| `src/types/internal_operation/pay_coin.rs` | 274 | PayCoin PTB building |
| `src/types/internal_operation/stake.rs` | 399 | Stake PTB building |
| `src/types/internal_operation/withdraw_stake.rs` | 162 | WithdrawStake PTB building |

## Appendix B: External Dependencies

| Dependency | Used For |
|------------|----------|
| `sui_rpc::client::Client` | gRPC client to Sui full node |
| `sui_types::transaction::TransactionData` | Building Sui transactions |
| `sui_types::transaction::ProgrammableTransaction` | PTB construction |
| `axum` | HTTP server framework |
| `bcs` | Binary Canonical Serialization |
| `fastcrypto` | Signature verification |
| `lru` | LRU cache for coin metadata |

## Appendix C: gRPC Calls Used

| gRPC Method | Used By | Purpose |
|-------------|---------|---------|
| `GetCheckpointRequest` | block, state, account | Fetch checkpoint/block data |
| `GetTransactionRequest` | block::transaction | Fetch single transaction |
| `GetBalanceRequest` | account, pay_sui, pay_coin, stake | Query balance |
| `GetEpochRequest` | network::status, lib::get_current_epoch | Current epoch |
| `GetServiceInfoRequest` | main, state | Chain ID, service info |
| `GetObjectRequest` | withdraw_stake | Fetch stake object refs |
| `GetCoinInfoRequest` | CoinMetadataCache | Coin metadata (symbol, decimals) |
| `ListOwnedObjectsRequest` | account (coins, fungible stake), withdraw_stake | List owned objects |
| `BatchGetObjectsRequest` | simulate_transaction | Fetch gas coin details |
| `SimulateTransactionRequest` | construction::submit, metadata | Dry run + gas estimation |
| `ExecuteTransactionRequest` | construction::submit | Execute transaction |
| `select_up_to_n_largest_coins` | pay_sui, pay_coin, stake | Coin selection (client helper) |
| `list_delegated_stake` | account (sub-accounts) | Staking info |
