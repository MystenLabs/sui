# TypeScript SDK 2.0 Documentation Patterns

Every rule below is sourced from https://sdk.mystenlabs.com. Source citations appear after each rule so there is no ambiguity about correctness.

---

## Rule 1: Client instantiation

### Wrong

```ts
import { SuiClient } from '@mysten/sui/client';
const client = new SuiClient({ url: 'https://fullnode.testnet.sui.io:443' });
```

### Right

```ts
import { SuiGrpcClient } from '@mysten/sui/grpc';

const client = new SuiGrpcClient({
  network: 'testnet',
  baseUrl: 'https://fullnode.testnet.sui.io:443',
});
```

### Also right (JSON-RPC, legacy only)

```ts
import { SuiJsonRpcClient, getJsonRpcFullnodeUrl } from '@mysten/sui/jsonRpc';

const client = new SuiJsonRpcClient({
  url: getJsonRpcFullnodeUrl('testnet'),
  network: 'testnet',
});
```

### Key details

- The gRPC constructor parameter is `baseUrl`, not `url`.
- The `network` parameter is required on all client constructors.
- There is no `getGrpcFullnodeUrl()` helper in the docs. Use direct URL strings for gRPC.
- `getJsonRpcFullnodeUrl()` exists only for the JSON-RPC client.

### Sources

- **[sui/clients/grpc.md](https://sdk.mystenlabs.com/sui/clients/grpc.md):** Constructor example uses `network` and `baseUrl` parameters.
- **[sui/migrations/sui-2.0/sui.md](https://sdk.mystenlabs.com/sui/migrations/sui-2.0/sui.md):** "`SuiClient` -> `SuiJsonRpcClient`", import path changed from `@mysten/sui/client` to `@mysten/sui/jsonRpc`. "Creating clients now demands specifying a `network` property."
- **[sui/migrations/sui-2.0/json-rpc-migration.md](https://sdk.mystenlabs.com/sui/migrations/sui-2.0/json-rpc-migration.md):** "SuiGrpcClient is recommended for most operations."

---

## Rule 2: Top-level client methods, not `.core`

### Wrong (in user-facing doc examples)

```ts
await client.core.getTransaction({ digest });
await client.core.getBalance({ owner, coinType });
await client.core.waitForTransaction({ digest });
await client.core.signAndExecuteTransaction({ transaction, signer });
await client.core.simulateTransaction({ transaction });
await client.core.executeTransaction({ transaction, signatures });
```

### Right

```ts
await client.getTransaction({ digest });
await client.getBalance({ owner, coinType });
await client.waitForTransaction({ digest });
await client.signAndExecuteTransaction({ transaction, signer });
await client.simulateTransaction({ transaction });
await client.executeTransaction({ transaction, signatures });
```

### When `.core` IS correct

Only in SDK/library code that accepts `ClientWithCoreApi` and must work across any transport (gRPC, JSON-RPC, GraphQL). Example:

```ts
// This is SDK code, not user code
class MySDK {
  constructor(private client: ClientWithCoreApi) {}
  async doSomething() {
    return this.client.core.getObject({ objectId: '0x...' });
  }
}
```

### Why this matters

The SDK has three API levels:
1. **Top-level client methods** (e.g., `client.getBalance()`) -- for user/application code
2. **Core API** (`client.core.*`) -- transport-agnostic interface for SDK/library authors
3. **Native service methods** (e.g., `client.ledgerService.*`) -- for advanced gRPC-specific operations

Documentation examples are user code. They should use level 1.

### Sources

- **[sui/clients.md](https://sdk.mystenlabs.com/sui/clients.md):** Two access levels are defined:
  - "**Native API**: Each client exposes full capabilities of its underlying transport"
  - "**Core API**: All clients implement a consistent interface through `client.core` for common operations. This standardized approach works identically across all transports, **making it essential for building SDKs** compatible with any client users choose."
- **[sui/sdk-building.md](https://sdk.mystenlabs.com/sui/sdk-building.md):** "All Sui SDKs should depend on `ClientWithCoreApi`." Shows `.core` used inside SDK class methods.
- **[sui/clients/core.md](https://sdk.mystenlabs.com/sui/clients/core.md):** "`ClientWithCoreApi Type`: Use this when building SDKs that should work with any transport."
- **[sui/clients/grpc.md](https://sdk.mystenlabs.com/sui/clients/grpc.md):** Shows `grpcClient.getCoins()` as a top-level method.
- **[sui/transactions/coins-and-balances.md](https://sdk.mystenlabs.com/sui/transactions/coins-and-balances.md):** Shows `grpcClient.getBalance()` directly (not `.core`).
- **[sui/transactions/signing-and-execution.md](https://sdk.mystenlabs.com/sui/transactions/signing-and-execution.md):** Shows `grpcClient.executeTransaction()` directly.

### Note on the migration guide

The JSON-RPC migration guide ([json-rpc-migration.md](https://sdk.mystenlabs.com/sui/migrations/sui-2.0/json-rpc-migration.md)) uses `.core` in its examples because it is documenting the Core API concept itself, not recommending `.core` for user code. Do not copy those examples verbatim into doc pages.

---

## Rule 3: Use `tx.coin()` and `tx.balance()`, not `splitCoins`

### Wrong

```ts
const [coin] = tx.splitCoins(tx.gas, [amount]);
tx.transferObjects([coin], recipient);
```

### Right: `tx.coin()` for Coin objects

```ts
// SUI (balance in MIST -- 1 SUI = 1,000,000,000 MIST)
tx.transferObjects(
  [tx.coin({ balance: 1_000_000_000n })],
  '0xRecipientAddress',
);

// Non-SUI coin type
tx.transferObjects(
  [tx.coin({ balance: 1_000_000n, type: '0xPackageId::module::CoinType' })],
  '0xRecipientAddress',
);
```

### Right: `tx.balance()` for Balance objects (Move function arguments)

```ts
tx.moveCall({
  target: '0xPackage::module::deposit',
  arguments: [
    tx.object('0xPoolId'),
    tx.balance({ balance: 1_000_000_000n }),
  ],
});
```

### Right: `balance::send_funds` for address balance transfers

```ts
tx.moveCall({
  target: '0x2::balance::send_funds',
  typeArguments: ['0x2::sui::SUI'],
  arguments: [
    tx.balance({ balance: 1_000_000_000n }),
    tx.pure.address('0xRecipientAddress'),
  ],
});
```

### Right: `coin::send_funds` for depositing coin objects into address balances

```ts
tx.moveCall({
  target: '0x2::coin::send_funds',
  typeArguments: ['0x2::sui::SUI'],
  arguments: [
    tx.object('0xMyCoinObjectId'),
    tx.pure.address('0xRecipientAddress'),
  ],
});
```

### API details

The `tx.coin()` and `tx.balance()` methods take an options object:

| Option | Type | Default | Purpose |
|--------|------|---------|---------|
| `balance` | `bigint \| number` | required | Amount in base units (MIST for SUI) |
| `type` | `string` | `0x2::sui::SUI` | Coin type |
| `useGasCoin` | `boolean` | `true` | For SUI, split from gas coin; set `false` for sponsored transactions |

There is also a standalone alias `coinWithBalance()`:

```ts
import { coinWithBalance, Transaction } from '@mysten/sui/transactions';

const tx = new Transaction();
tx.transferObjects([coinWithBalance({ balance: 1_000_000_000 })], recipient);
```

### When `splitCoins` IS appropriate

Only in doc examples that specifically teach manual coin operations or low-level PTB construction. Not in payment flow examples, transfer examples, or any "how to send tokens" doc.

### How resolution works

At build time, the SDK resolver replaces `tx.coin()` / `tx.balance()` intents with concrete commands:
- Prefers address balances (avoids versioned object dependencies)
- Falls back to fetching coin objects, merging, and splitting
- Zero-balance requests resolve to `balance::zero` / `coin::zero` without network lookups

### Sources

- **[sui/transactions/coins-and-balances.md](https://sdk.mystenlabs.com/sui/transactions/coins-and-balances.md):**
  - "**Primary Methods: `tx.coin()` and `tx.balance()`** -- These are the recommended approaches for obtaining tokens in transactions, automatically drawing from both systems."
  - Shows `balance::send_funds` and `coin::send_funds` patterns.
  - "The resolver prefers address balances to avoid versioned object dependencies."
  - Manual `splitCoins` shown separately under "Manual Coin Operations."

---

## Rule 4: Keypair creation in examples

### Wrong (when the keypair needs to sign transactions)

```ts
const adminKeypair = new Ed25519Keypair();
// Creates a random, unfunded address -- transactions will fail
```

### Right

```ts
import { Ed25519Keypair } from '@mysten/sui/keypairs/ed25519';

// From Bech32 secret key (suiprivkey1...)
const keypair = Ed25519Keypair.fromSecretKey(process.env.ADMIN_SECRET_KEY!);

// From mnemonic
const keypair = Ed25519Keypair.deriveKeypair(mnemonic);
```

### Right (guarded loader for production examples)

```ts
function loadKeypair(): Ed25519Keypair {
  const key = process.env.ADMIN_SECRET_KEY;
  if (!key) throw new Error('ADMIN_SECRET_KEY environment variable is required');
  return Ed25519Keypair.fromSecretKey(key);
}
```

### When `new Ed25519Keypair()` IS appropriate

Only in examples that explicitly demonstrate key generation (like an agent wallet setup page). Never when the keypair is subsequently used to sign transactions that require gas.

### Sources

- **[sui/cryptography/keypairs.md](https://sdk.mystenlabs.com/sui/cryptography/keypairs.md):**
  - "Instantiating a new keypair class generates a random key pair."
  - "The `fromSecretKey` method reconstructs a keypair from stored key material."
  - "The Sui TypeScript SDK supports deriving a key pair from a mnemonic phrase" via `Ed25519Keypair.deriveKeypair()`.

---

## Rule 5: `FailedTransaction` is onchain

### Wrong

```ts
// "Returns null if the network has never seen it"
if (result.$kind === 'FailedTransaction') {
  return null; // WRONG -- this IS onchain, gas WAS charged
}
```

### Right

```ts
if (result.$kind === 'FailedTransaction') {
  // Transaction IS onchain. Sender was charged gas. Move execution aborted.
  // The transaction has effects, just not the intended ones.
  const error = result.FailedTransaction.effects.status.error;
}

if (result.$kind === 'Transaction') {
  // Transaction succeeded with intended effects
}
```

### Three distinct outcomes to handle

1. **`Transaction`** -- succeeded onchain with intended effects
2. **`FailedTransaction`** -- landed onchain, gas charged, Move execution aborted. Has a digest, has effects. Should NOT be retried (would double-charge gas).
3. **Not found** (error/exception) -- transaction was never seen by the network or hasn't been indexed yet. May be retried.

### Separate Move aborts from system failures

In circuit breaker or retry logic, distinguish:
- **Move abort** (e.g., insufficient balance, access denied) -- a business-logic rejection, not a system failure
- **Network/system failure** (timeout, node unreachable) -- a transient failure worth retrying or tripping a breaker for

A `FailedTransaction` is typically a Move abort. Treating it as a system failure leads to incorrect breaker behavior.

### Sources

- **[sponsor/basic-usage.md](https://sdk.mystenlabs.com/sponsor/basic-usage.md):** Three outcomes explicitly defined:
  - "`Rejected`: Policy declined, no execution occurred"
  - "`FailedTransaction`: **Executed on-chain but aborted** (sponsor still pays gas)"
  - "`Transaction`: Successful execution"
- **[sui/transactions/signing-and-execution.md](https://sdk.mystenlabs.com/sui/transactions/signing-and-execution.md):**
  - "Results are discriminated unions--check `result.$kind` for `'FailedTransaction'` or successful variants."
- **[sui/clients/core.md](https://sdk.mystenlabs.com/sui/clients/core.md):**
  - "For transactions, always verify `Transaction` vs `FailedTransaction` properties."
- **[sui/migrations/sui-2.0/json-rpc-migration.md](https://sdk.mystenlabs.com/sui/migrations/sui-2.0/json-rpc-migration.md):**
  - `const tx = result.Transaction ?? result.FailedTransaction;` -- confirms both variants have effects.

---

## Rule 6: `waitForTransaction` before error handling

### Wrong

```ts
const result = await client.signAndExecuteTransaction({ transaction, signer });
if (result.$kind === 'FailedTransaction') {
  throw new Error(result.FailedTransaction.effects.status.error);
}
await client.waitForTransaction({ digest: result.Transaction.digest });
```

### Right

```ts
const result = await client.signAndExecuteTransaction({ transaction, signer });
await client.waitForTransaction(result); // wait first, pass result directly

if (result.$kind === 'FailedTransaction') {
  throw new Error(result.FailedTransaction.effects.status.error);
}
```

### Key details

- Call `waitForTransaction` before acting on the result (ensures finality).
- Pass the result object directly instead of extracting the digest from one of two discriminated union branches.

### Sources

- **[sui/transactions/signing-and-execution.md](https://sdk.mystenlabs.com/sui/transactions/signing-and-execution.md):**
  - "Use `waitForTransaction()` to ensure read APIs reflect changes before subsequent operations."
- **Reviewer guidance (hayes-mysten):** Wait before error handling; pass result directly instead of extracting digest. Not yet in SDK docs but consistent with the SDK team's recommendation.

---

## Rule 7: Sign on the client, not the signer

### Wrong

```ts
const result = await signer.signAndExecuteTransaction({ transaction });
```

### Right

```ts
const result = await client.signAndExecuteTransaction({
  transaction,
  signer: keypair,
});
```

### Sources

- **[sui/transactions/signing-and-execution.md](https://sdk.mystenlabs.com/sui/transactions/signing-and-execution.md):**
  - "The simplest approach. The keypair signs the transaction and submits it to the network in one call." Shows `client.signAndExecuteTransaction()` with `signer` as a parameter.
- **[sui/sdk-building.md](https://sdk.mystenlabs.com/sui/sdk-building.md):**
  - "Always use `signAndExecuteTransaction` to enable wallet integration, transaction sponsorship, and custom signing flows."

---

## Rule 8: Discriminated union for results

### Wrong

```ts
const error = result.effects.status.error; // old v1 flat structure
```

### Right

```ts
if (result.$kind === 'FailedTransaction') {
  const error = result.FailedTransaction.effects.status.error;
}
```

### Sources

- **[sui/transactions/signing-and-execution.md](https://sdk.mystenlabs.com/sui/transactions/signing-and-execution.md):**
  - "Results are discriminated unions--check `result.$kind`"
- **[sui/migrations/sui-2.0/json-rpc-migration.md](https://sdk.mystenlabs.com/sui/migrations/sui-2.0/json-rpc-migration.md):**
  - Before: `result.effects?.status?.status`; After: `tx.effects.status.success`

---

## Rule 9: GraphQL variables, not string interpolation

### Wrong

```ts
const query = `{
  transactionBlocks(filter: { sentAddress: "${senderAddress}" }) { ... }
}`;
```

### Right

```ts
const query = `
  query PaymentHistory($sender: SuiAddress!) {
    transactionBlocks(filter: { sentAddress: $sender }) { ... }
  }
`;
const result = await graphqlClient.query({
  query,
  variables: { sender: senderAddress },
});
```

### Sources

- **[sui/migrations/sui-2.0/json-rpc-migration.md](https://sdk.mystenlabs.com/sui/migrations/sui-2.0/json-rpc-migration.md):** Every single GraphQL example in the migration guide uses parameterized variables. Examples use `variables: { digests: [...] }`, `variables: { id: '0x...', version: 42 }`, `variables: { owner: '0xabc...' }`, etc. None use string interpolation.

---

## Rule 10: `Transaction.from()`, not `TransactionDataBuilder`

### Wrong

```ts
import { TransactionDataBuilder } from '@mysten/sui/transactions';
const finalData = TransactionDataBuilder.fromBytes(finalBytes);
```

### Right

```ts
import { Transaction } from '@mysten/sui/transactions';
const tx = Transaction.from(finalBytes);
```

### Sources

- **[sui/transactions/basics.md](https://sdk.mystenlabs.com/sui/transactions/basics.md):**
  - Shows `Transaction` as the public API: `import { Transaction } from '@mysten/sui/transactions';`
  - "Serialization to/from JSON via `tx.toJSON()` and `Transaction.from()`"
- `TransactionDataBuilder` is not mentioned in any public-facing docs page. It is an internal class.

---

## Rule 11: Gas station -- use the sponsor SDK

### Wrong

Building a 350+ line custom server with manual coin pool management, rate limiting, reservation tracking.

### Right

```ts
import {
  createSponsor,
  defaults,
  gasBudget,
  allowedFunctions,
} from '@mysten-incubation/sponsor';

const sponsor = createSponsor({
  signer: Ed25519Keypair.fromSecretKey(process.env.SPONSOR_KEY!),
  client,
  validate: [
    defaults(),
    gasBudget({ max: 50_000_000n }),
    allowedFunctions(['0xabc::shop::buy']),
  ],
});
```

### Executing a sponsored transaction

```ts
const result = await sponsor.signAndExecuteTransaction({
  transaction: txBytes,       // Uint8Array, base64, Transaction, or JSON
  userSignature,              // NOT `signature` — the parameter is `userSignature`
});

if (result.$kind === 'Rejected') {
  // Policy declined. No execution, no gas charged.
  console.error(result.Rejected.reason);
} else if (result.$kind === 'FailedTransaction') {
  // Onchain but aborted. Sponsor still pays gas. Do NOT retry.
  console.error(result.FailedTransaction.effects.status.error);
} else {
  // Success.
  console.log(result.Transaction.digest);
}
```

### Key details

- Sponsors pay from address balance using `setGasPayment([])`.
- This avoids locking specific coin objects, enabling parallel transaction execution.
- `defaults()` bundles 8 validators: `validSender`, `onlyAddressBalanceGas`, `gasCoinNotUsed`, `onlySenderWithdrawals`, `userSignatureMatchesSender`, `gasBudget`, `simulationSucceeds`, `boundedExpiration`.
- Result is a discriminated union: `Rejected` | `FailedTransaction` | `Transaction`.
- **The parameter is `userSignature`, NOT `signature`.** This is an easy mistake — always use `userSignature`.

### Built-in validators

| Validator | Rejects when |
|-----------|-------------|
| `validSender()` | sender unset or is gas owner |
| `onlyAddressBalanceGas()` | gas payment isn't empty |
| `gasCoinNotUsed()` | command uses gas coin |
| `onlySenderWithdrawals()` | FundsWithdrawal isn't sender's |
| `userSignatureMatchesSender()` | user signature invalid or wrong signer |
| `gasBudget({ min?, max? })` | budget unset or out of range |
| `allowedPackages([...])` | MoveCall targets unlisted package |
| `allowedFunctions([...])` | MoveCall targets unlisted function |
| `simulationSucceeds()` | dry-run indicates failure |
| `boundedExpiration()` | expiration missing or beyond next epoch |

### Sources

- **[sponsor.md](https://sdk.mystenlabs.com/sponsor.md):**
  - "As a sponsor operator, you supply a `Signer` and validation policy, then `createSponsor` processes user transactions"
  - "Sponsors pay from **address balance** using empty gas payment (`setGasPayment([])`)"
  - "This approach avoids locking specific coin objects, enabling parallel transaction execution"
- **[sponsor/basic-usage.md](https://sdk.mystenlabs.com/sponsor/basic-usage.md):**
  - "Prefer services that expect **already-built bytes plus the user's signature**."
  - Three-outcome result: Rejected, FailedTransaction, Transaction.
- **[sponsor/validators.md](https://sdk.mystenlabs.com/sponsor/validators.md):** Full validator table, custom validator pattern with `createAnalyzer()`.

---

## Rule 12: Sponsored transactions -- `useGasCoin: false`

### Wrong

```ts
// In a sponsored transaction where gas coin belongs to sponsor:
tx.transferObjects([tx.coin({ balance: 100n })], recipient);
// Default useGasCoin: true will try to split from sponsor's gas coin
```

### Right

```ts
tx.transferObjects(
  [tx.coin({ balance: 100n, useGasCoin: false })],
  recipient,
);
```

### Sources

- **[sui/transactions/coins-and-balances.md](https://sdk.mystenlabs.com/sui/transactions/coins-and-balances.md):**
  - "For sponsored transactions where gas coin belongs to sponsor:"
  - Shows `tx.coin({ balance: 100n, useGasCoin: false })`.
- **[sponsor/basic-usage.md](https://sdk.mystenlabs.com/sponsor/basic-usage.md):**
  - "When users spend their own SUI, use `useGasCoin: false` to source funds from their balance rather than gas coins."

---

## Rule 13: Gasless stablecoin transfers

### Pattern

```ts
const USDC = '0xdba34672e30cb065b1f93e3ab55318768fd6fef66c15942c9f7cb846e2f900e7::usdc::USDC';

const tx = new Transaction();
tx.setSender(keypair.toSuiAddress());

tx.moveCall({
  target: '0x2::balance::send_funds',
  typeArguments: [USDC],
  arguments: [
    tx.balance({ type: USDC, balance: 1_000_000 }),
    tx.pure.address(recipient),
  ],
});

const result = await client.signAndExecuteTransaction({
  transaction: tx,
  signer: keypair,
});
```

### Key details

- With gRPC or GraphQL, qualifying transactions are automatically detected and gas price is set at build.
- JSON-RPC does NOT auto-detect; requires manual `tx.setGasPrice(0)`.
- Restricted to allowlisted stablecoins and `balance::send_funds` PTB shape.

### Sources

- **[sui/transactions/coins-and-balances.md](https://sdk.mystenlabs.com/sui/transactions/coins-and-balances.md):**
  - "Gasless transactions enable peer-to-peer payments of qualified stablecoins without SUI gas fees."
  - "Using `0x2::balance::send_funds` with `tx.balance()` represents the recommended SDK approach."
  - "With gRPC or GraphQL transports, qualifying transactions are automatically detected and gas price is set at build."
  - JSON-RPC caveat: "Manual opt-in requires setting gas price to zero with `tx.setGasPrice(0)` after confirming coin type allowlisting and PTB shape eligibility."

---

## Rule 14: Never log secret keys

### Wrong

```ts
console.log('Secret:', keypair.getSecretKey());
```

### Right

```ts
console.log('Address:', keypair.toSuiAddress());
// Never log the secret key -- not even in documentation examples
```

### Sources

- **[sui/cryptography/keypairs.md](https://sdk.mystenlabs.com/sui/cryptography/keypairs.md):** Documents `getSecretKey()` for key export but no example in the entire SDK docs calls `console.log` with it.

---

## Rule 15: Client extensions for ecosystem packages

### Wrong

```ts
const name = await client.core.defaultNameServiceName({ address });
```

### Right

```ts
import { suins } from '@mysten/suins';

const client = new SuiGrpcClient({
  network: 'mainnet',
  baseUrl: 'https://fullnode.mainnet.sui.io:443',
}).$extend(suins());

const name = await client.suins.getName('0xabc...');
```

### Sources

- **[sui/migrations/sui-2.0/json-rpc-migration.md](https://sdk.mystenlabs.com/sui/migrations/sui-2.0/json-rpc-migration.md):** Shows `$extend(deepbook(...), suins())` pattern.
- **[sui/clients.md](https://sdk.mystenlabs.com/sui/clients.md):** "All support extensions through the `$extend` method."

---

## Rule 16: Schema validation, not manual typeof checks

### Wrong

```ts
if (
  typeof sponsored.txBytes !== 'string' ||
  typeof sponsored.signature !== 'string' ||
  typeof sponsored.gasOwner !== 'string'
) {
  throw new Error('Invalid response');
}
```

### Right

```ts
import { z } from 'zod';

const SponsoredResponseSchema = z.object({
  txBytes: z.string(),
  signature: z.string(),
  gasOwner: z.string(),
});

const sponsored = SponsoredResponseSchema.parse(responseJson);
```

### Sources

- **Reviewer guidance (hayes-mysten):** "we should probably just use a schema validation library, this feels like a bad way to do validation." Not in SDK docs, but a best practice from the SDK team.

---

## Rule 17: Differentiate HTTP error status codes

### Wrong

```ts
try {
  // ... logic
} catch (error) {
  res.status(400).json({ error: (error as Error).message });
}
```

### Right

```ts
try {
  // ... logic
} catch (error) {
  if (error instanceof ValidationError) {
    res.status(400).json({ error: error.message });
  } else if (error instanceof AuthorizationError) {
    res.status(403).json({ error: error.message });
  } else {
    res.status(500).json({ error: 'Internal server error' });
  }
}
```

### Sources

- **Reviewer guidance (hayes-mysten):** "this error handling looks overly broad for a 400 error." Not in SDK docs, but a web development best practice.

---

## Rule 18: Codegen for type-safe Move calls (optional)

### Without codegen

```ts
tx.moveCall({
  target: `${PACKAGE_ID}::spending_mandate::create_mandate`,
  arguments: [tx.pure.u64(amount), tx.pure.u64(interval)],
});
```

### With codegen

```ts
import { createMandate } from './generated/spending_mandate';

createMandate(tx, { amount, interval });
```

### Key details

- Package: `@mysten/codegen` (install as devDependency).
- Config file: `sui-codegen.config.ts`.
- Status: "currently in development and might have breaking changes."
- Consider whether codegen is appropriate for documentation examples given its development status.

### Sources

- **[codegen.md](https://sdk.mystenlabs.com/codegen.md):**
  - "automatically generates type-safe TypeScript code from your Move packages"
  - "Type-safe function wrappers: Creates TypeScript functions with full type safety"
  - "currently in development and might have breaking changes"

---

## Rule 19: Transaction default expiration

### Know this

In SDK 2.0, transactions automatically set expiration to current epoch + 1. You don't need to set it manually unless overriding.

### Sources

- **[sui/migrations/sui-2.0/sui.md](https://sdk.mystenlabs.com/sui/migrations/sui-2.0/sui.md):**
  - "Transactions now automatically set expiration to 'current epoch + 1 using ValidDuring' for replay protection, though this can be overridden."

---

## Quick decision table

| What you're doing | Use |
|---|---|
| Send SUI to an address | `tx.coin({ balance })` + `tx.transferObjects()` |
| Send non-SUI token | `tx.coin({ balance, type })` + `tx.transferObjects()` |
| Send to address balance | `0x2::balance::send_funds` + `tx.balance()` |
| Deposit coin object to address balance | `0x2::coin::send_funds` + `tx.object()` |
| Pass Balance to Move function | `tx.balance({ balance })` |
| Gasless stablecoin transfer | `0x2::balance::send_funds` + `tx.balance({ type })` |
| Sponsor a transaction | `@mysten-incubation/sponsor` `createSponsor()` |
| User-code query (getBalance, etc.) | Top-level `client.getBalance()` |
| SDK/library query | `client.core.getBalance()` with `ClientWithCoreApi` |
| Deserialize transaction bytes | `Transaction.from(bytes)` |
| Generate Move call bindings | `@mysten/codegen` |
| Resolve SuiNS names | `client.$extend(suins())` |

---

## Verification status

| Rule | Verified in SDK docs | Source page |
|------|---------------------|-------------|
| 1. Client instantiation | Yes | grpc.md, sui-2.0/sui.md |
| 2. Top-level vs .core | Yes | clients.md, sdk-building.md, core.md |
| 3. tx.coin/tx.balance | Yes | coins-and-balances.md |
| 4. Keypair creation | Yes | keypairs.md |
| 5. FailedTransaction | Yes | basic-usage.md, signing-and-execution.md |
| 6. waitForTransaction ordering | Partial (reviewer guidance) | signing-and-execution.md |
| 7. Sign on client | Yes | signing-and-execution.md, sdk-building.md |
| 8. Discriminated union | Yes | signing-and-execution.md, json-rpc-migration.md |
| 9. GraphQL variables | Yes | json-rpc-migration.md |
| 10. Transaction.from() | Yes | basics.md |
| 11. Sponsor SDK | Yes | sponsor.md, basic-usage.md, validators.md |
| 12. useGasCoin: false | Yes | coins-and-balances.md, basic-usage.md |
| 13. Gasless stablecoins | Yes | coins-and-balances.md |
| 14. No secret key logging | Yes (implicit) | keypairs.md |
| 15. Client extensions | Yes | json-rpc-migration.md, clients.md |
| 16. Schema validation | Reviewer guidance only | -- |
| 17. HTTP error codes | Reviewer guidance only | -- |
| 18. Codegen | Yes (but "in development") | codegen.md |
| 19. Default expiration | Yes | sui-2.0/sui.md |
