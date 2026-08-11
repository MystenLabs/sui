/*
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
*/

/**
 * Replaces the generic templated goal.description on index/landing pages
 * ("Reader gets a clear overview of X and knows which subtopic to read next")
 * with hand-written, content-grounded reader outcomes.
 *
 * Usage:
 *   node scripts/rewrite-index-descriptions.mjs          # dry run
 *   node scripts/rewrite-index-descriptions.mjs --apply  # write changes
 */

import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import matter from 'gray-matter';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const CONTENT_ROOT = path.resolve(__dirname, '..', '..', 'content');
const dryRun = !process.argv.includes('--apply');

const FIXES = {
  'develop.mdx':
    'Reader can orient in the developer essentials — architecture, writing Move packages, and building transactions — and pick where to start',
  'develop/accessing-data/index.mdx':
    'Reader can choose the right mechanism (GraphQL, gRPC, indexers, archival store) to access Sui transactions, checkpoints, objects, and events',
  'develop/accessing-data/archival-store/index.mdx':
    'Reader understands how the Archival Store and Service provide scalable access to historical onchain data beyond full-node retention',
  'develop/accessing-data/custom-indexer/index.mdx':
    'Reader understands the sui-indexer-alt-framework interfaces (process, commit) for building custom high-performance Sui indexers',
  'develop/accessing-data/graphql/index.mdx':
    'Reader understands how the GraphQL RPC Service queries Sui across the indexer, archival store, and full nodes, and when to use it over gRPC',
  'develop/accessing-data/grpc/index.mdx':
    'Reader understands how the full node gRPC API uses Protocol Buffers for high-performance, type-safe access to Sui data',
  'develop/cryptography/index.mdx':
    'Reader understands Sui cryptographic agility and the algorithms and primitives available to smart contracts and applications',
  'develop/manage-packages/index.mdx':
    'Reader can manage Move packages safely, understanding how the UpgradeCap governs future upgrades and how to verify dependency package IDs',
  'develop/objects/index.mdx':
    'Reader can navigate the Sui object model, object usage, and the object ownership types',
  'develop/objects/display/index.mdx':
    'Reader understands the Object Display standard (V2 via sui::display_registry) for managing offchain representation of a type onchain',
  'develop/objects/object-ownership/index.mdx':
    'Reader can distinguish the object ownership types (address-owned, shared, party, immutable) and how each affects transaction access and versioning',
  'develop/objects/transfers/index.mdx':
    'Reader can choose the right object transfer mechanism for their use case on Sui',
  'develop/publish-upgrade-packages/index.mdx':
    'Reader understands what a Move package is and how modules are compiled and published to a Sui network',
  'develop/security/index.mdx':
    'Reader can apply Sui security best practices, including treating shared objects as non-authorization boundaries in privileged code paths',
  'develop/sui-architecture/index.mdx':
    'Reader can navigate Sui architecture topics — the blockchain, its transactions, and the validators',
  'develop/testing-debugging/index.mdx':
    'Reader can choose the right tools and techniques to test and debug Move smart contracts and applications on Sui',
  'develop/transaction-payment/index.mdx':
    'Reader understands how a Sui transaction pays for both computational execution and long-term object storage',
  'develop/transactions/index.mdx':
    'Reader understands that every Sui update happens through a transaction, and can find the right transaction topic for their task',
  'develop/transactions/ptbs/index.mdx':
    'Reader understands what programmable transaction blocks are and how their commands compose into a single transaction',
  'develop/transactions/transaction-auth/index.mdx':
    'Reader can compare the transaction authentication methods available on Sui and choose one',
  'develop/write-move/index.mdx':
    'Reader can get oriented in writing Move packages for Sui — syntax fundamentals, best practices, and package structure',
  'getting-started.mdx':
    'Reader can pick a starting point for building on Sui — an agent skill, the Hello, World! example, or another example app',
  'getting-started/examples/index.mdx':
    'Reader can browse end-to-end example apps (Move contracts, React frontends, security challenges) and pick one matching their goal',
  'getting-started/onboarding/index.mdx':
    'Reader can start the Sui onboarding path and understand what building on Sui involves',
  'onchain-finance.mdx':
    'Reader can find the right onchain finance topic on Sui — digital assets, custody, tokenomics, and DeepBook',
  'onchain-finance/asset-custody/index.mdx':
    'Reader can find the right asset-custody approach on Sui across fungible tokens, closed-loop tokens, and tokenized assets',
  'onchain-finance/asset-custody/address-balances/index.mdx':
    'Reader understands how address balances replace the UTXO-style Coin model with address-owned balances that remove coin-selection complexity',
  'onchain-finance/asset-custody/wallets/index.mdx':
    'Reader understands how Sui wallets store keys and sign transactions, and can compare Slush, self-custodial, and zkLogin wallet types',
  'onchain-finance/closed-loop-token/index.mdx':
    'Reader can use the Closed-Loop Token standard to restrict which apps use a token and define custom transfer, spend, and conversion policies',
  'onchain-finance/examples-patterns/index.mdx':
    'Reader can browse common asset patterns on Sui and reference the one that fits their use case',
  'onchain-finance/fungible-tokens/index.mdx':
    'Reader can find how to create and use fungible tokens on Sui',
  'onchain-finance/kiosk/index.mdx':
    'Reader understands how Kiosk enables commerce apps on Sui and how Kiosk apps extend it without changing core functionality',
  'onchain-finance/pas/index.mdx':
    'Reader understands how the Permissioned Asset Standard enforces restricted asset movement through Accounts, Policies, and approval logic',
  'onchain-finance/tokenized-assets/index.mdx':
    'Reader can find how to design and extend NFTs on Sui, including soulbound tokens, rental mechanics, and asset tokenization',
  'operators.mdx':
    'Reader can find the right guide for running Sui infrastructure — full nodes, validators, bridge nodes, data management, and exchange integration',
  'operators/index.mdx':
    'Operator can find guidance for running a full node, operating as a validator, or integrating SUI into an exchange',
  'operators/data-management/index.mdx':
    'Operator can set up and manage data indexing, archival storage, and remote data stores for Sui nodes',
  'operators/full-node/index.mdx':
    'Operator can find the guides needed to set up and operate a Sui full node',
  'operators/validator/index.mdx':
    'Operator can find the guides needed to run and manage a Sui validator node',
  'references.mdx':
    'Reader can look up low-level Sui reference details across features and architecture',
  'references/ide/index.mdx':
    'Reader can find the IDE tools and extensions for Move development on Sui, including language server and debugging support',
  'sui-stack.mdx':
    'Reader can get oriented in the Sui Stack components and primitives and find the one relevant to their app',
  'sui-stack/messaging/index.mdx':
    'Reader understands how the Messaging SDK delivers end-to-end encrypted group messaging using Seal encryption, a Walrus relayer, and onchain permissions',
  'sui-stack/nautilus/index.mdx':
    'Reader understands how Nautilus runs offchain logic in TEEs and verifies it onchain to trigger safe smart contract workflows',
  'sui-stack/on-chain-primitives/index.mdx':
    'Reader understands the native onchain time and randomness primitives Sui contracts can use without external oracles',
  'sui-stack/seal/index.mdx':
    'Reader understands how Seal provides threshold encryption with onchain access control enforced by Move-defined policies',
  'sui-stack/suins/index.mdx':
    'Reader understands how SuiNS maps human-readable .sui names to addresses, with resolution, reverse lookup, and subnames',
  'sui-stack/suiplay0x1/index.mdx':
    'Game developer understands how to build for the SuiPlay0X1 handheld gaming device and what it supports',
  'sui-stack/walrus/index.mdx':
    'Reader understands how Walrus provides decentralized storage for large binary files, coordinated and paid for through Sui',
  'sui-stack/zklogin-integration/index.mdx':
    'Reader can implement the zkLogin flow — ephemeral keys, JWT, user salt, and zero-knowledge proof — to enable zkLogin transactions in an app',
};

let applied = 0;
let missing = 0;

for (const [relPath, newDesc] of Object.entries(FIXES)) {
  const filePath = path.join(CONTENT_ROOT, relPath);
  if (!fs.existsSync(filePath)) {
    console.error(`WARNING: not found: ${relPath}`);
    missing++;
    continue;
  }
  const raw = fs.readFileSync(filePath, 'utf8');
  const { data, content: body } = matter(raw);
  if (!data.goal) {
    console.error(`WARNING: no goal on ${relPath}`);
    continue;
  }
  const oldDesc = data.goal.description;
  if (oldDesc === newDesc) continue;

  if (dryRun) {
    console.log(relPath);
    console.log(`  OLD: ${oldDesc}`);
    console.log(`  NEW: ${newDesc}\n`);
  } else {
    data.goal.description = newDesc;
    fs.writeFileSync(filePath, matter.stringify(body, data), 'utf8');
  }
  applied++;
}

console.log(`${'─'.repeat(50)}`);
console.log(`${dryRun ? 'DRY RUN' : 'APPLIED'}  rewritten=${applied} missing=${missing}`);
if (dryRun) console.log('Run with --apply to write changes.');
