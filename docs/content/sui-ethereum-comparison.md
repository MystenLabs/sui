# Sui vs Ethereum Comparison

| Topic | Sui | Ethereum |
|-------|-----|----------|
| Digital Signature Algorithm | Ed25519, secp256k1 secp256r1 | secp256k1 |
| Consensus Mechanism | DPoS | PoS |
| VM and its language(s) | MoveVM, Move Lang | EVM, Solidity, Vyper |
| Chain Datastructure | DAG | Blocks |
| Common standards (coin, token, nft, etc) | Coin/Token | ERC-20, ERC-721, ERC-1155 |
| Coin names, name of the smallest unit (mist?) | Sui, Mist | ETH, Wei |
| Available frameworks for development, foundry, sui cli | SUI CLI | Foundry, hardhat |
| L1/L2 | No L2, relies on fast L1 | Many L2s |
| Governance | Onchain Governance | EIP + Node Operator consensus |
| Bridges | Supported | Supported |
| Network Security (how much to take over how much staking) | 66% stake | 51% stake |
| Smart Contract auditing (sui requires less due to move lang, eth more expensive) | Less auditing required, language does some of the lifting (object model) | Solidity provides less protection requiring greater auditing |
| Private transactions | no? | Public by design, L2/3rd party support |
| TVL | 1 billion | 46 Billion |
| Languages implemented in, clients / server | Rust, Typescript | Many.. |
| Eventing | Indexed by topic | Indexed by sender, object id, type, timestamp |
| Indexing | High level tx data + objects, coins, etc | High level tx data |
| Oracles | 3rd party? | 3rd party |
| Network upgrade strategy | Protocol flags and framework upgrades are voted on by validators then enabled. | EIPs + Hardforking, no on-chain mechanism |
| IDE | VSCode | Many |
| Transaction Lifecycle | Two round trips from client to validators to generate a transaction certificate (guaranteeing execution) another round trip for shared objects to ensure ordering. Very low latency | Transaction gossiped to network, verified added to mempool, validators select transactions from mempool. Random validator proposes a block, other validators vote yes/no on block. After a sufficient number of blocks have passed a transaction is considered final. High latency due to block height requirement for "finality" |
| Account vs object-centric models | Object ownership inherent to SUI, objects are first class citizens and encompass everything "owned" on Sui | Custom ownership logic written within contracts typically using "mappings". Only ethereum coins are first class citizens with global APIs. All ownership APIs are contract specific. |
| Parallel Execution vs ethereum serial execution, fast path | Transactions which can be parallel are run in parallel | Every transaction is sequentially run |
| UTXO (do we call it UTXO) vs account based coins | UTXO + accounts (soon) | Accounts |
| Storage fees, storage rebates, storage accounts to pay for fees over time | Low, rebates on destroying objects | High, no rebates |
| Fee cost | Low (<1$?) | High (>1$) |
| Contract publish cost | Low (<1$?) | High (>100$) |
| Contract immutability | Native mutable/immutable support using upgrade capabilities | Not native, requires auditing the solidity code deployed. Can be discerned by some op codes. |
| Contract upgrading | Native, upgrade capability mediated | Achieved using "proxy" pattern to "delegate" calls. Upgrades change where calls are directed to. |
| Composability | Call any number of functions within a single transaction using PTBs. Compose by taking the output of one contract call and passing it into another. Ensures atomic execution | Each call is its own transaction which must be processed individually and serialized by the chain which requires careful publishing to ensure execution. Not atomic. |
| Token royalties | Enforced by the chain | Only enforceable by marketplaces |
| ABI vs PTB | Runtime PTB construction | Compile time ABI interface |
