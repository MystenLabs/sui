# Transfer and Pay Methods in TransactionBuilder

This document describes the step-by-step implementation of transfer and pay methods in `crates/sui-transaction-builder/src/lib.rs`.

## Address Balance Support

Address balances can be used as a funding source by passing **coin reservation objects** to these methods. Callers are responsible for:

1. Creating a `FundsWithdrawalArg` to reserve the required funds from their address balance
2. Using `coin::redeem_funds` to convert the withdrawal into a coin object
3. Passing the resulting coin object (or its ObjectID) to the transaction builder methods

The transaction builder treats coin reservation objects the same as regular coin objects. This approach gives callers full control over when and how address balances are used.

## 1. `transfer_object` (lines 98-121)

Transfers a single object to a recipient. The object must allow public transfers.

**Steps:**
1. **Create a ProgrammableTransactionBuilder** - Initialize an empty transaction builder
2. **Get full object reference** - Fetch the object from storage and compute its full reference (ID, version, digest, and owner info)
3. **Build transfer command** - Call `builder.transfer_object(recipient, full_obj_ref)` which:
   - Creates a pure input for the recipient address
   - Adds the object as an input (handling shared vs owned object args)
   - Emits a `Command::TransferObjects` command
4. **Get reference gas price** - Fetch the current gas price from the network
5. **Select gas coin** - Find a suitable gas coin from the signer's owned coins that:
   - Is not the object being transferred
   - Has sufficient balance for the gas budget
   - If `gas` is provided, use that coin directly
6. **Create TransactionData** - Package everything into a signed transaction

---

## 2. `transfer_sui` (lines 144-157)

Transfers SUI coin to a recipient. The SUI object is also used as the gas object.

**Steps:**
1. **Get object reference** - Fetch the SUI coin's reference (ID, version, digest)
2. **Get reference gas price** - Fetch the current gas price
3. **Create transaction via `TransactionData::new_transfer_sui`** which internally:
   - Creates a ProgrammableTransactionBuilder
   - Calls `builder.transfer_sui(recipient, amount)` which:
     - If `amount` is Some: Splits that amount from GasCoin, then transfers the split coin
     - If `amount` is None: Transfers the entire GasCoin
   - Uses the SUI object as both the coin source and gas payment

---

## 3. `pay` (lines 171-197)

Sends `Coin<T>` to multiple recipients with specified amounts. Uses a separate gas object.

**Steps:**
1. **Validate gas not in input coins** - Ensure the gas coin is not in the list of input coins (fails if it is)
2. **Get coin references** - Fetch object references for all input coins in parallel
3. **Get reference gas price** - Fetch the current gas price
4. **Select gas coin** - Find a gas coin not in the input list
5. **Create transaction via `TransactionData::new_pay`** which internally:
   - Creates a ProgrammableTransactionBuilder
   - Calls `builder.pay(coins, recipients, amounts)` which:
     - **Merge all input coins into the first coin** - If multiple coins, emit `Command::MergeCoins` to combine them
     - **Split the merged coin** - Emit `Command::SplitCoins` with one split per amount
     - **Transfer split coins** - Group recipients (to minimize transfers if same recipient appears multiple times) and emit `Command::TransferObjects` for each unique recipient
   - Uses the separate gas object for gas payment

---

## 4. `pay_sui` (lines 222-248)

Sends SUI coins to multiple recipients. The first input coin is used as gas.

**Steps:**
1. **Validate input not empty** - Ensure at least one coin is provided
2. **Get coin references** - Fetch object references for all input coins
3. **Extract gas coin** - Remove the first coin to use as gas payment
4. **Get reference gas price** - Fetch the current gas price
5. **Create transaction via `TransactionData::new_pay_sui`** which:
   - Re-inserts the gas coin at position 0 of the coins vector (so all coins are available)
   - Creates a ProgrammableTransactionBuilder
   - Calls `builder.pay_sui(recipients, amounts)` which:
     - **Uses GasCoin as the source** - The runtime provides access to all input coins via `Argument::GasCoin`
     - **Split coins** - Emit `Command::SplitCoins(GasCoin, amounts)`
     - **Transfer split coins** - Emit `Command::TransferObjects` for each unique recipient
   - After execution: first coin holds residual balance (sum(inputs) - sum(amounts) - gas_cost), other coins are deleted

---

## 5. `pay_all_sui` (lines 257-281)

Sends all SUI from input coins to a single recipient. The first input coin is used as gas.

**Steps:**
1. **Validate input not empty** - Ensure at least one coin is provided
2. **Get coin references** - Fetch object references for all input coins
3. **Extract gas coin** - Remove the first coin to use as gas payment
4. **Get reference gas price** - Fetch the current gas price
5. **Create transaction via `TransactionData::new_pay_all_sui`** which:
   - Re-inserts the gas coin at position 0 of the coins vector
   - Creates a ProgrammableTransactionBuilder
   - Calls `builder.pay_all_sui(recipient)` which:
     - **Transfer entire GasCoin** - Emits `Command::TransferObjects([GasCoin], recipient)`
   - After execution: The runtime first merges all input coins into the gas coin, then transfers it to the recipient (minus gas cost). All other input coins are deleted.

---

## 6. `split_coin` (lines 664-694)

Splits a coin into multiple coins with specified amounts.

**Steps:**
1. **Get coin object** - Fetch the coin and its reference
2. **Get coin type** - Extract the coin's type arguments (e.g., `Coin<SUI>` -> `SUI`)
3. **Get reference gas price** - Fetch the current gas price
4. **Select gas coin** - Find a gas coin that is not the coin being split
5. **Create Move call transaction** - Calls `sui::pay::split_vec(coin, amounts)`:
   - Input 0: The coin object
   - Input 1: BCS-encoded vector of split amounts
   - Newly split coins are created; the original coin keeps the remainder

---

## 7. `split_coin_equal` (lines 697-727)

Splits a coin into N equal-sized coins.

**Steps:**
1. **Get coin object** - Fetch the coin and its reference
2. **Get coin type** - Extract the coin's type arguments
3. **Get reference gas price** - Fetch the current gas price
4. **Select gas coin** - Find a gas coin that is not the coin being split
5. **Create Move call transaction** - Calls `sui::pay::split_n(coin, count)`:
   - Input 0: The coin object
   - Input 1: BCS-encoded split count
   - Creates N coins of equal value from the original

---

## 8. `merge_coins` (lines 755-792)

Merges two coins into one.

**Steps:**
1. **Get primary coin** - Fetch the target coin object and its reference
2. **Get coin to merge** - Fetch the source coin's reference
3. **Get coin type** - Extract the coin's type arguments
4. **Get reference gas price** - Fetch the current gas price
5. **Select gas coin** - Find a gas coin that is neither the primary nor the coin being merged
6. **Create Move call transaction** - Calls `sui::pay::join(target, source)`:
   - Input 0: Primary coin (receives the balance)
   - Input 1: Coin to merge (gets destroyed)
   - The primary coin's balance increases; the merged coin is deleted

---

## Key Helper: `select_gas` (lines 56-85)

Used by most methods to find a suitable gas coin.

**Steps:**
1. **Validate gas budget** - Ensure budget >= reference gas price
2. **If gas provided** - Use the specified gas object directly
3. **If gas not provided** - Query all owned `GasCoin` objects from the signer, find the first one that:
   - Is not in the list of input objects for this transaction
   - Has balance >= gas budget
4. **Error if no suitable gas** - Suggest using `pay-sui` or `transfer-sui` if no separate gas coin is available

---

## Summary Table

| Method | Gas Source | Coin Merging | Use Case |
|--------|-----------|--------------|----------|
| `transfer_object` | Separate gas coin | N/A | Transfer any object |
| `transfer_sui` | Same as transfer coin | N/A | Transfer SUI (simple) |
| `pay` | Separate gas coin | Yes (input coins merged) | Pay multiple recipients with any coin type |
| `pay_sui` | First input coin | Yes (via runtime) | Pay multiple recipients with SUI |
| `pay_all_sui` | First input coin | Yes (via runtime) | Send all SUI to one recipient |
| `split_coin` | Separate gas coin | N/A | Split coin by amounts |
| `split_coin_equal` | Separate gas coin | N/A | Split coin into N equal parts |
| `merge_coins` | Separate gas coin | Yes (2 coins) | Combine two coins |

## Related Files

- `crates/sui-transaction-builder/src/lib.rs` - Main implementation
- `crates/sui-types/src/programmable_transaction_builder.rs` - Low-level PTB commands
- `crates/sui-types/src/transaction.rs` - TransactionData constructors
- `crates/sui-json-rpc/src/transaction_builder_api.rs` - RPC wrapper
