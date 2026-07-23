/*
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
*/

/**
 * One-time fix for goal.description strings that the template pipeline
 * produced with grammatical breakage (verbatim frontmatter pastes and
 * mid-sentence truncation). Replaces them with hand-written, reader-focused
 * outcomes.
 *
 * Usage:
 *   node scripts/fix-goal-descriptions.mjs          # dry run
 *   node scripts/fix-goal-descriptions.mjs --apply  # write changes
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
  'develop/accessing-data/authenticated-events.mdx':
    'Reader understands how authenticated events let a light client cryptographically verify Move events without trusting an intermediary',
  'develop/accessing-data/grpc/what-is-grpc.mdx':
    'Reader understands how gRPC provides fast, type-safe access to Sui network data',
  'develop/cryptography/ecvrf.mdx':
    'Reader understands how ECVRF generates a random number with a proof that it was produced using a secret key',
  'develop/objects/transfers/custom-rules.mdx':
    'Reader can define custom transfer rules that must be satisfied before Sui considers a transfer valid',
  'develop/transactions/transaction-auth/address-aliases.mdx':
    'Reader can use address aliases to configure which keys are allowed to sign transactions for an address',
  'getting-started/examples/scenario-testing.mdx':
    'Reader can write multi-transaction tests with test_scenario that simulate flows across multiple users and shared objects',
  'getting-started/onboarding/install-source.mdx':
    'Reader can build and install the Sui framework from source, either locally or directly from GitHub',
  'onchain-finance/deepbook-margin/contract-information/margin-manager.mdx':
    'Reader understands how the margin manager enables leveraged trading on DeepBook',
  'onchain-finance/deepbookv3/contract-information/flash-loans.mdx':
    'Reader can use DeepBookV3 flash loans to borrow and repay within a single programmable transaction block',
  'onchain-finance/examples-patterns/loyalty-tokens.mdx':
    'Reader can use the Closed-Loop Token standard to create tokens valid only within specific workflows, such as loyalty tokens',
  'onchain-finance/examples-patterns/nft-rental.mdx':
    'Reader can implement NFT rental that lets users rent NFTs under a defined policy instead of owning them outright',
  'onchain-finance/fungible-tokens/coin.mdx':
    'Reader understands how the Coin standard supports creating a broad range of fungible tokens on Sui',
  'operators/data-management/remote-store-setup.mdx':
    'Operator can run the checkpoint blob indexer to populate a remote store with protobuf checkpoint blobs',
  'references/cli.mdx':
    'Reader can look up the Sui CLI command groups for interacting with the network and the Move language',
  'references/fullnode-protocol.mdx':
    'Reader can look up the Sui full node gRPC protocol available on all full nodes',
  'references/research-papers.mdx':
    'Reader can look up Sui-relevant research papers co-authored by Sui team members',
  'references/rust-sdk.mdx':
    'Reader can look up how to use the Sui Rust SDK to interact with Sui networks in Rust',
  'references/sui-api.mdx':
    'Reader can look up how SuiJSON aligns JSON inputs with Move call arguments',
  'references/sui-graphql.mdx':
    'Reader can look up how to use the GraphQL RPC service to interact with the Sui network',
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
console.log(`${dryRun ? 'DRY RUN' : 'APPLIED'}  fixed=${applied} missing=${missing}`);
if (dryRun) console.log('Run with --apply to write changes.');
