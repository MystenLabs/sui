/*
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
*/

/**
 * Applies content-grounded rewrites to the goal.description strings that the
 * quality eval scored <= 2 (broken grammar, truncation, or scope mismatch).
 * Descriptions are hand-written from each page's actual headings and intro.
 *
 * Usage:
 *   node scripts/fix-low-score-descriptions.mjs          # dry run
 *   node scripts/fix-low-score-descriptions.mjs --apply  # write changes
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
  'develop/objects/versioning.mdx':
    'Reader understands how Sui versions objects by (ID, version) and the fastpath versus consensus versioning paths',
  'develop/sui-architecture/protocol-upgrades.mdx':
    'Reader understands how Sui ships protocol and framework upgrades that validators adopt in lockstep',
  'develop/transactions/transaction-auth/auth-overview.mdx':
    'Reader understands cryptographic keys, addresses, and signatures on Sui',
  'onchain-finance/asset-custody/address-balances/using-address-balances.mdx':
    'Reader can send, withdraw, pay gas from, and query address balances using the TypeScript SDK, CLI, and Move',
  'onchain-finance/payment-kit.mdx':
    'Reader understands the Payment Kit standard for secure payment processing with registries, receipts, and duplicate prevention',
  'develop/accessing-data/custom-indexer/custom-indexers.mdx':
    'Reader understands what custom indexers are, when to use them, and how the sui-indexer-alt-framework ingestion, processing, and storage layers fit together',
  'develop/accessing-data/using-events.mdx':
    'Reader can define, emit, and query Move events to track onchain activity from offchain applications',
  'develop/publish-upgrade-packages/deploy.mdx':
    'Reader can compile and publish a Move package to a Sui network',
  'develop/sui-architecture/checkpoint-verification.mdx':
    'Reader can verify checkpoints and understands checkpoint commitments',
  'develop/sui-architecture/sui-security.mdx':
    "Reader understands Sui's security guarantees for asset owners, from ownership and finality to auditing and censorship resistance",
  'develop/transactions/ptbs/building-ptb.mdx':
    'Reader can build programmable transaction blocks with the TypeScript SDK and CLI, including gas configuration and offline building',
  'develop/transactions/transaction-lifecycle.mdx':
    "Reader understands each stage of a transaction's lifecycle on Sui, from creation through consensus, finality, and checkpoints",
  'onchain-finance/closed-loop-token/action-request.mdx':
    'Reader understands how an ActionRequest authorizes protected token actions and how to confirm one',
  'onchain-finance/deepbook-margin/contract-information/risk-ratio.mdx':
    'Reader understands how risk ratios determine leverage limits and collateral requirements in DeepBook Margin',
  'onchain-finance/deepbook-margin/margin-risks.mdx':
    'Reader understands the risks of margin trading on DeepBook, including liquidation and interest rate fluctuations',
  'onchain-finance/deepbookv3/contract-information/query-the-pool.mdx':
    'Reader can query pool state such as orders, balances, and quantities via the DeepBookV3 pool read API',
  'onchain-finance/examples-patterns/kiosk.mdx':
    'Reader can use the Kiosk standard to join tokenized assets while enforcing transfer policies',
  'onchain-finance/fungible-tokens/integrating-with-stablecoins.mdx':
    'Reader learns what stablecoins are and where they are used on Sui',
  'onchain-finance/fungible-tokens/sui-bridging.mdx':
    'Reader can bridge tokens to and from Sui using Sui Bridge and Wormhole, and understands their limits and supported assets',
  'onchain-finance/kiosk/kiosk-example.mdx':
    'Reader can open and configure a Sui Kiosk and understands its guarantees for owners, buyers, marketplaces, and creators',
  'onchain-finance/payments.mdx':
    'Reader can integrate payment flows on Sui, from reading and managing balances to sponsoring gasless transactions',
  'operators/data-management/managing-data.mdx':
    'Operator understands data management on Sui full nodes and can configure pruning and archival policies to optimize their node',
  'references/contribute/contribute-to-sui-repos.mdx':
    'Contributor can find how to open issues, fork, and submit PRs and SIPs to Sui repositories',
  'references/contribute/contribution-process.mdx':
    'Contributor can edit Sui docs via the GitHub web editor or a local environment and understands the review process',
  'references/contribute/localize-sui-docs.mdx':
    'Contributor learns that Sui docs are localized through Crowdin',
  'references/contribute/mdx-components.mdx':
    'Contributor can use the custom MDX components available in Sui docs, such as tabs, admonitions, and ImportContent',
  'references/gaming.mdx':
    'Game developer can learn how to use Sui features such as dynamic NFTs, Kiosk, soulbound assets, and onchain randomness to build games',
  'references/ptb-commands.mdx':
    "Reader can look up each PTB command's form, return type, and signature",
  'references/sui-api/rpc-best-practices.mdx':
    'Reader can apply RPC best practices when configuring RPC provider settings',
  'references/sui-move.mdx':
    'Reader can find links to Move language references, the Move Book, and Sui framework docs',
  'references/ts-asset-tokenization.mdx':
    'Reader can look up how the tokenized_asset module represents real-world assets as onchain fractional tokens',
  'sui-stack/suiplay0x1/best-practices.mdx':
    'Reader can apply best practices for transaction handling, gas, and data storage when developing for SuiPlay0X1',
  'sui-stack/suiplay0x1/migration-strategies.mdx':
    'Reader can support wallet and asset migration flows between on-device and off-device play in the Sui gaming ecosystem',
  'sui-stack/walrus/indexer-walrus.mdx':
    'Reader can build a custom indexer for a blog platform backed by Walrus content-addressable storage',
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
