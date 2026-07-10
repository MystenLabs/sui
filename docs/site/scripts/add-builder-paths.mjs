/*
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
*/

/**
 * Adds builder_paths frontmatter to pages referenced in builder paths.
 *
 * Pages from the evals dashboard have eval status (covered/partial/missing).
 * Additional relevant pages have eval: null (not yet evaluated).
 *
 * Usage:
 *   node scripts/add-builder-paths.mjs          # dry run
 *   node scripts/add-builder-paths.mjs --apply  # write changes
 */

import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import matter from 'gray-matter';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const CONTENT_ROOT = path.resolve(__dirname, '..', '..', 'content');
const dryRun = !process.argv.includes('--apply');

// ─── Builder path definitions ───────────────────────────────────────────────
//
// SOURCE OF TRUTH
//   Path definitions, step ordering, and eval status are transcribed from the
//   builder-path evals dashboard:
//     https://docs-analytics-dashboard.vercel.app/evals
//   Snapshot date: 2026-07-10. The per-path `score` fields below are the
//   dashboard coverage scores at that snapshot, retained for provenance even
//   though they are not written into page frontmatter.
//
//   This table is a manual snapshot and WILL drift from the dashboard. When
//   updating, re-pull from the dashboard and bump the snapshot date above so
//   the frontmatter's origin stays verifiable.
//
// eval values:
//   'covered'  – evaluated, fully documented (from dashboard)
//   'partial'  – evaluated, has gaps (from dashboard)
//   'missing'  – evaluated, no adequate docs (from dashboard)
//   null       – not yet evaluated in dashboard

const BUILDER_PATHS_SOURCE = {
  dashboard: 'https://docs-analytics-dashboard.vercel.app/evals',
  snapshotDate: '2026-07-10',
};

const BUILDER_PATHS = [
  {
    id: 'defi-deepbook',
    name: 'DeFi / DeepBook',
    score: 35,
    steps: [
      // ── Setup ──
      { step: 'Install Sui CLI', stage: 'Setup', page: 'getting-started/onboarding/sui-install.mdx', eval: 'covered' },
      { step: 'Scaffold frontend', stage: 'Setup', page: 'getting-started/examples/dapp-kit-frontend.mdx', eval: 'covered' },
      { step: 'TypeScript SDK', stage: 'Setup', page: null, eval: 'covered', note: 'External: sdk.mystenlabs.com' },
      { step: 'Developer tools', stage: 'Setup', page: 'getting-started/tooling.mdx', eval: null },
      // ── DeFi Primitives ──
      { step: 'Understand DeFi on Sui', stage: 'DeFi Primitives', page: 'onchain-finance/deepbookv3/deepbook.mdx', eval: 'partial' },
      { step: 'DeepBook architecture', stage: 'DeFi Primitives', page: 'onchain-finance/deepbookv3/design.mdx', eval: null },
      { step: 'DeepBook integration', stage: 'DeFi Primitives', page: 'onchain-finance/deepbookv3/deepbook.mdx', eval: 'partial' },
      { step: 'DeepBook SDK overview', stage: 'DeFi Primitives', page: 'onchain-finance/deepbookv3-sdk/deepbookv3-sdk.mdx', eval: null },
      { step: 'Choose integration model', stage: 'DeFi Primitives', page: null, eval: 'missing' },
      { step: 'DEEP fees and funding', stage: 'DeFi Primitives', page: 'onchain-finance/deepbookv3/contract-information/staking-governance.mdx', eval: null },
      { step: 'Order constraints', stage: 'DeFi Primitives', page: 'onchain-finance/deepbookv3/contract-information/orders.mdx', eval: null },
      { step: 'Pool creation', stage: 'DeFi Primitives', page: 'onchain-finance/deepbookv3/contract-information/permissionless-pool.mdx', eval: null },
      { step: 'Pool querying', stage: 'DeFi Primitives', page: 'onchain-finance/deepbookv3/contract-information/query-the-pool.mdx', eval: null },
      { step: 'Swap mechanics', stage: 'DeFi Primitives', page: 'onchain-finance/deepbookv3/contract-information/swaps.mdx', eval: null },
      { step: 'Balance manager', stage: 'DeFi Primitives', page: 'onchain-finance/deepbookv3/contract-information/balance-manager.mdx', eval: null },
      { step: 'Flash loans', stage: 'DeFi Primitives', page: 'onchain-finance/deepbookv3/contract-information/flash-loans.mdx', eval: null },
      { step: 'Referral system', stage: 'DeFi Primitives', page: 'onchain-finance/deepbookv3/contract-information/referral.mdx', eval: null },
      // ── DeFi SDK ──
      { step: 'SDK: Orders', stage: 'DeFi SDK', page: 'onchain-finance/deepbookv3-sdk/orders.mdx', eval: null },
      { step: 'SDK: Swaps', stage: 'DeFi SDK', page: 'onchain-finance/deepbookv3-sdk/swaps.mdx', eval: null },
      { step: 'SDK: Pools', stage: 'DeFi SDK', page: 'onchain-finance/deepbookv3-sdk/pools.mdx', eval: null },
      { step: 'SDK: Balance manager', stage: 'DeFi SDK', page: 'onchain-finance/deepbookv3-sdk/balance-manager.mdx', eval: null },
      { step: 'SDK: Flash loans', stage: 'DeFi SDK', page: 'onchain-finance/deepbookv3-sdk/flash-loans.mdx', eval: null },
      { step: 'SDK: Staking & governance', stage: 'DeFi SDK', page: 'onchain-finance/deepbookv3-sdk/staking-governance.mdx', eval: null },
      // ── Margin ──
      { step: 'Margin trading overview', stage: 'Margin', page: 'onchain-finance/deepbook-margin/deepbook-margin.mdx', eval: null },
      { step: 'Margin architecture', stage: 'Margin', page: 'onchain-finance/deepbook-margin/design.mdx', eval: null },
      { step: 'Margin risks', stage: 'Margin', page: 'onchain-finance/deepbook-margin/margin-risks.mdx', eval: null },
      { step: 'Margin contracts', stage: 'Margin', page: 'onchain-finance/deepbook-margin/contract-information.mdx', eval: null },
      { step: 'Margin contract: Orders', stage: 'Margin', page: 'onchain-finance/deepbook-margin/contract-information/orders.mdx', eval: null },
      { step: 'Margin contract: Manager', stage: 'Margin', page: 'onchain-finance/deepbook-margin/contract-information/margin-manager.mdx', eval: null },
      { step: 'Margin SDK overview', stage: 'Margin', page: 'onchain-finance/deepbook-margin-sdk/deepbook-margin-sdk.mdx', eval: null },
      { step: 'Margin SDK: Orders', stage: 'Margin', page: 'onchain-finance/deepbook-margin-sdk/orders.mdx', eval: null },
      { step: 'Margin SDK: Manager', stage: 'Margin', page: 'onchain-finance/deepbook-margin-sdk/margin-manager.mdx', eval: null },
      { step: 'Margin SDK: Pool', stage: 'Margin', page: 'onchain-finance/deepbook-margin-sdk/margin-pool.mdx', eval: null },
      { step: 'Margin SDK: TP/SL', stage: 'Margin', page: 'onchain-finance/deepbook-margin-sdk/tpsl.mdx', eval: null },
      { step: 'Margin SDK: Maintainer', stage: 'Margin', page: 'onchain-finance/deepbook-margin-sdk/maintainer.mdx', eval: null },
      // ── Move Contract ──
      { step: 'Custom coin creation', stage: 'Move Contract', page: 'onchain-finance/fungible-tokens/create-a-fungible-token-coin.mdx', eval: 'covered' },
      { step: 'TradeProof lifecycle', stage: 'Move Contract', page: null, eval: 'missing' },
      { step: 'OTW factory pattern', stage: 'Move Contract', page: null, eval: 'missing' },
      { step: 'Shared objects', stage: 'Move Contract', page: 'develop/objects/object-ownership/shared.mdx', eval: null },
      // ── Advanced ──
      { step: 'PTB composition examples', stage: 'Advanced', page: 'develop/transactions/ptbs/prog-txn-blocks.mdx', eval: 'missing' },
      { step: 'Oracle integration', stage: 'Advanced', page: null, eval: 'missing' },
      { step: 'DeepBook event indexing', stage: 'Advanced', page: 'onchain-finance/deepbookv3/deepbookv3-indexer.mdx', eval: null },
      { step: 'Margin event indexing', stage: 'Advanced', page: 'onchain-finance/deepbook-margin/deepbook-margin-indexer.mdx', eval: null },
      { step: 'Event querying', stage: 'Advanced', page: 'develop/accessing-data/using-events.mdx', eval: null },
      { step: 'Authenticated events', stage: 'Advanced', page: 'develop/accessing-data/authenticated-events.mdx', eval: null },
      // ── UX ──
      { step: 'zkLogin + sponsored tx', stage: 'UX', page: 'sui-stack/zklogin-integration/zklogin.mdx', eval: 'missing' },
      // ── Operations ──
      { step: 'Upgrade and versioning', stage: 'Operations', page: 'develop/publish-upgrade-packages/upgrade.mdx', eval: 'partial' },
      { step: 'Package versioning', stage: 'Operations', page: 'develop/publish-upgrade-packages/versioning.mdx', eval: null },
      { step: 'Package deployment', stage: 'Operations', page: 'develop/publish-upgrade-packages/deploy.mdx', eval: null },
      { step: 'DeFi security patterns', stage: 'Operations', page: 'develop/security/best-practices.mdx', eval: null },
      { step: 'Transaction auth', stage: 'Operations', page: 'develop/transactions/transaction-auth/auth-overview.mdx', eval: null },
      { step: 'Multisig', stage: 'Operations', page: 'develop/transactions/transaction-auth/multisig.mdx', eval: null },
      { step: 'Signature verification', stage: 'Operations', page: 'develop/cryptography/signing.mdx', eval: null },
      // ── Agent Readiness ──
      { step: 'DeFi agent skill', stage: 'Agent Readiness', page: null, eval: 'missing' },
    ],
  },
  {
    id: 'p2p-payments',
    name: 'P2P Payments / Neomoney',
    score: 27,
    steps: [
      // ── Setup ──
      { step: 'Install Sui CLI', stage: 'Setup', page: 'getting-started/onboarding/sui-install.mdx', eval: 'covered' },
      { step: 'Scaffold frontend', stage: 'Setup', page: 'getting-started/examples/dapp-kit-frontend.mdx', eval: 'covered' },
      { step: 'Connect a frontend', stage: 'Setup', page: 'getting-started/onboarding/app-frontends.mdx', eval: null },
      { step: 'Developer tools', stage: 'Setup', page: 'getting-started/tooling.mdx', eval: null },
      { step: 'Choose payment model', stage: 'Setup', page: null, eval: 'missing' },
      // ── Auth ──
      { step: 'zkLogin overview', stage: 'Auth', page: 'sui-stack/zklogin-integration/index.mdx', eval: null },
      { step: 'zkLogin setup', stage: 'Auth', page: 'sui-stack/zklogin-integration/zklogin.mdx', eval: 'partial' },
      { step: 'zkLogin example', stage: 'Auth', page: 'sui-stack/zklogin-integration/zklogin-example.mdx', eval: 'partial' },
      { step: 'OpenID provider config', stage: 'Auth', page: 'sui-stack/zklogin-integration/developer-account.mdx', eval: null },
      { step: 'Passkey authentication', stage: 'Auth', page: 'develop/cryptography/passkeys.mdx', eval: null },
      { step: 'Salt management', stage: 'Auth', page: null, eval: 'missing' },
      { step: 'Session lifecycle', stage: 'Auth', page: null, eval: 'missing' },
      // ── Wallets & Funding ──
      { step: 'What is a wallet', stage: 'Wallets & Funding', page: 'onchain-finance/asset-custody/wallets/what-is-a-wallet.mdx', eval: null },
      { step: 'Self-custody wallets', stage: 'Wallets & Funding', page: 'onchain-finance/asset-custody/wallets/self-custody.mdx', eval: null },
      { step: 'Wallet Standard', stage: 'Wallets & Funding', page: 'onchain-finance/asset-custody/wallets/wallet-standard.mdx', eval: null },
      { step: 'zkLogin wallets', stage: 'Wallets & Funding', page: 'onchain-finance/asset-custody/wallets/zk-login-wallets.mdx', eval: null },
      { step: 'Address balances', stage: 'Wallets & Funding', page: 'onchain-finance/asset-custody/address-balances/index.mdx', eval: null },
      { step: 'Using address balances', stage: 'Wallets & Funding', page: 'onchain-finance/asset-custody/address-balances/using-address-balances.mdx', eval: null },
      { step: 'Wallet funding', stage: 'Wallets & Funding', page: null, eval: 'missing' },
      // ── Gas ──
      { step: 'Gas mechanics', stage: 'Gas', page: 'develop/transaction-payment/gas-in-sui.mdx', eval: null },
      { step: 'Sponsored transactions', stage: 'Gas', page: 'develop/transaction-payment/sponsor-txn.mdx', eval: 'covered' },
      { step: 'Gasless stablecoin transfers', stage: 'Gas', page: 'develop/transaction-payment/gasless-stablecoin-transfers.mdx', eval: null },
      { step: 'Gas smashing', stage: 'Gas', page: 'develop/transaction-payment/gas-smashing.mdx', eval: null },
      { step: 'Local fee markets', stage: 'Gas', page: 'develop/transaction-payment/local-fee-markets.mdx', eval: null },
      { step: 'Self-hosted gas station', stage: 'Gas', page: null, eval: 'missing' },
      // ── Tokens ──
      { step: 'Coin standard', stage: 'Tokens', page: 'onchain-finance/fungible-tokens/coin.mdx', eval: null },
      { step: 'Currency standard', stage: 'Tokens', page: 'onchain-finance/fungible-tokens/currency.mdx', eval: null },
      { step: 'Create a token', stage: 'Tokens', page: 'onchain-finance/fungible-tokens/create-a-fungible-token.mdx', eval: null },
      { step: 'Stablecoin integration', stage: 'Tokens', page: 'onchain-finance/fungible-tokens/integrating-with-stablecoins.mdx', eval: null },
      { step: 'Regulated tokens', stage: 'Tokens', page: 'onchain-finance/fungible-tokens/regulated-tokens.mdx', eval: null },
      // ── Payment Flow ──
      { step: 'Payment intents', stage: 'Payment Flow', page: 'onchain-finance/payment-intents.mdx', eval: null },
      { step: 'Build payment transaction', stage: 'Payment Flow', page: 'onchain-finance/payment-kit.mdx', eval: 'partial' },
      { step: 'Transaction overview', stage: 'Payment Flow', page: 'develop/transactions/txn-overview.mdx', eval: null },
      { step: 'Transaction lifecycle', stage: 'Payment Flow', page: 'develop/transactions/transaction-lifecycle.mdx', eval: null },
      { step: 'Recipient resolution (SuiNS)', stage: 'Payment Flow', page: 'sui-stack/suins/index.mdx', eval: 'missing' },
      { step: 'SuiNS integration', stage: 'Payment Flow', page: 'sui-stack/suins/sui-stack-suins.mdx', eval: null },
      { step: 'Address-owned objects', stage: 'Payment Flow', page: 'develop/objects/object-ownership/address-owned.mdx', eval: null },
      { step: 'Money math pitfalls', stage: 'Payment Flow', page: null, eval: 'missing' },
      // ── Signing ──
      { step: 'Auth overview', stage: 'Signing', page: 'develop/transactions/transaction-auth/auth-overview.mdx', eval: null },
      { step: 'Intent signing', stage: 'Signing', page: 'develop/transactions/transaction-auth/intent-signing.mdx', eval: null },
      { step: 'Offline signing', stage: 'Signing', page: 'develop/transactions/transaction-auth/offline-signing.mdx', eval: null },
      { step: 'Signature verification', stage: 'Signing', page: 'develop/cryptography/signing.mdx', eval: null },
      // ── History ──
      { step: 'Data access interfaces', stage: 'History', page: 'develop/accessing-data/data-serving.mdx', eval: null },
      { step: 'Transaction indexing (migration)', stage: 'History', page: 'develop/accessing-data/json-rpc-migration.mdx', eval: 'partial' },
      { step: 'GraphQL queries', stage: 'History', page: 'develop/accessing-data/graphql/query-with-graphql.mdx', eval: null },
      { step: 'gRPC queries', stage: 'History', page: 'develop/accessing-data/grpc/using-grpc.mdx', eval: null },
      { step: 'Event querying', stage: 'History', page: 'develop/accessing-data/using-events.mdx', eval: null },
      { step: 'Authenticated events', stage: 'History', page: 'develop/accessing-data/authenticated-events.mdx', eval: null },
      { step: 'Custom indexers', stage: 'History', page: 'develop/accessing-data/custom-indexer/custom-indexers.mdx', eval: null },
      { step: 'Build a custom indexer', stage: 'History', page: 'develop/accessing-data/custom-indexer/build.mdx', eval: null },
      { step: 'Archival store', stage: 'History', page: 'develop/accessing-data/archival-store/what-is-archival-store.mdx', eval: null },
      // ── Operations ──
      { step: 'Mainnet cutover', stage: 'Operations', page: null, eval: 'missing' },
      { step: 'Payment security', stage: 'Operations', page: 'develop/security/best-practices.mdx', eval: null },
      { step: 'Regulatory considerations', stage: 'Compliance', page: null, eval: 'missing' },
      // ── Agent Readiness ──
      { step: 'Payments agent skill', stage: 'Agent Readiness', page: null, eval: 'missing' },
      { step: 'End-to-end P2P app', stage: 'Reference App', page: null, eval: 'missing' },
    ],
  },
  {
    id: 'agentic-payments',
    name: 'Agentic Payments',
    score: 48,
    steps: [
      // ── Environment ──
      { step: 'Install toolchain', stage: 'Environment', page: 'getting-started/onboarding/sui-install.mdx', eval: 'covered' },
      { step: 'Object model (ownership, shared)', stage: 'Environment', page: 'develop/sui-architecture/object-model.mdx', eval: 'covered' },
      { step: 'Address-owned objects', stage: 'Environment', page: 'develop/objects/object-ownership/address-owned.mdx', eval: null },
      { step: 'Wrapped objects', stage: 'Environment', page: 'develop/objects/object-ownership/wrapped.mdx', eval: null },
      { step: 'Immutable objects', stage: 'Environment', page: 'develop/objects/object-ownership/immutable.mdx', eval: null },
      { step: 'Dynamic fields', stage: 'Environment', page: 'develop/objects/dynamic-fields.mdx', eval: null },
      { step: 'Coin/Balance representation', stage: 'Environment', page: 'onchain-finance/fungible-tokens/coin.mdx', eval: 'partial' },
      { step: 'Agent identity & key custody', stage: 'Environment', page: null, eval: 'partial' },
      // ── Authorization ──
      { step: 'Auth overview', stage: 'Authorization', page: 'develop/transactions/transaction-auth/auth-overview.mdx', eval: null },
      { step: 'Agent wallet/signer setup', stage: 'Authorization', page: null, eval: 'partial' },
      { step: 'Multisig for agent security', stage: 'Authorization', page: 'develop/transactions/transaction-auth/multisig.mdx', eval: null },
      { step: 'Offline signing', stage: 'Authorization', page: 'develop/transactions/transaction-auth/offline-signing.mdx', eval: null },
      { step: 'Signature verification', stage: 'Authorization', page: 'develop/cryptography/signing.mdx', eval: null },
      { step: 'Spending policy/mandate', stage: 'Authorization', page: null, eval: 'missing' },
      { step: 'Transfer policies', stage: 'Authorization', page: 'develop/objects/transfers/transfer-policies.mdx', eval: null },
      { step: 'Sponsored gas', stage: 'Authorization', page: 'develop/transaction-payment/sponsor-txn.mdx', eval: 'partial' },
      { step: 'Gas mechanics', stage: 'Authorization', page: 'develop/transaction-payment/gas-in-sui.mdx', eval: null },
      { step: 'Gas smashing', stage: 'Authorization', page: 'develop/transaction-payment/gas-smashing.mdx', eval: null },
      { step: 'Gasless stablecoin transfers', stage: 'Authorization', page: 'develop/transaction-payment/gasless-stablecoin-transfers.mdx', eval: null },
      { step: 'zkLogin for agent identity', stage: 'Authorization', page: 'sui-stack/zklogin-integration/index.mdx', eval: null },
      // ── Money Movement ──
      { step: 'Construct payment PTB', stage: 'Money Movement', page: 'develop/transactions/ptbs/building-ptb.mdx', eval: 'covered' },
      { step: 'PTB inputs and results', stage: 'Money Movement', page: 'develop/transactions/ptbs/inputs-and-results.mdx', eval: null },
      { step: 'Payment intents', stage: 'Money Movement', page: 'onchain-finance/payment-intents.mdx', eval: null },
      { step: 'Transaction overview', stage: 'Money Movement', page: 'develop/transactions/txn-overview.mdx', eval: null },
      { step: 'Transaction lifecycle', stage: 'Money Movement', page: 'develop/transactions/transaction-lifecycle.mdx', eval: null },
      { step: 'Pay-per-request protocol (HTTP 402)', stage: 'Money Movement', page: null, eval: 'missing' },
      { step: 'Recurring/streaming payments', stage: 'Money Movement', page: null, eval: 'missing' },
      { step: 'Verify & reflect settlement', stage: 'Money Movement', page: null, eval: 'missing' },
      // ── Ship ──
      { step: 'Test full agentic flow', stage: 'Ship', page: null, eval: 'partial' },
      { step: 'Index payment history', stage: 'Ship', page: 'develop/accessing-data/json-rpc-migration.mdx', eval: 'partial' },
      { step: 'Data access interfaces', stage: 'Ship', page: 'develop/accessing-data/data-serving.mdx', eval: null },
      { step: 'GraphQL queries', stage: 'Ship', page: 'develop/accessing-data/graphql/query-with-graphql.mdx', eval: null },
      { step: 'gRPC queries', stage: 'Ship', page: 'develop/accessing-data/grpc/using-grpc.mdx', eval: null },
      { step: 'Event querying', stage: 'Ship', page: 'develop/accessing-data/using-events.mdx', eval: null },
      { step: 'Authenticated events', stage: 'Ship', page: 'develop/accessing-data/authenticated-events.mdx', eval: null },
      { step: 'Custom indexers', stage: 'Ship', page: 'develop/accessing-data/custom-indexer/custom-indexers.mdx', eval: null },
      { step: 'Build a custom indexer', stage: 'Ship', page: 'develop/accessing-data/custom-indexer/build.mdx', eval: null },
      { step: 'Indexer data integration', stage: 'Ship', page: 'develop/accessing-data/custom-indexer/indexer-data-integration.mdx', eval: null },
      { step: 'Archival store', stage: 'Ship', page: 'develop/accessing-data/archival-store/what-is-archival-store.mdx', eval: null },
      { step: 'Harden (revocation, retries, idempotency)', stage: 'Ship', page: null, eval: 'missing' },
      { step: 'Deploy to mainnet', stage: 'Ship', page: 'develop/publish-upgrade-packages/upgrade.mdx', eval: 'covered' },
      { step: 'Package versioning', stage: 'Ship', page: 'develop/publish-upgrade-packages/versioning.mdx', eval: null },
      { step: 'Package deployment', stage: 'Ship', page: 'develop/publish-upgrade-packages/deploy.mdx', eval: null },
      { step: 'Security best practices', stage: 'Ship', page: 'develop/security/best-practices.mdx', eval: null },
    ],
  },
  {
    id: 'walrus-general',
    name: 'Walrus General Data Storage',
    score: 91,
    steps: [
      // ── Environment ──
      { step: 'Install toolchain', stage: 'Environment', page: 'getting-started/onboarding/sui-install.mdx', eval: 'covered' },
      { step: 'Walrus concepts', stage: 'Environment', page: 'sui-stack/walrus/sui-stack-walrus.mdx', eval: 'covered' },
      { step: 'Storage resource, epochs & lifetime', stage: 'Environment', page: 'sui-stack/walrus/sui-stack-walrus.mdx', eval: 'covered' },
      // ── Store ──
      { step: 'Choose upload path', stage: 'Store', page: 'sui-stack/walrus/sui-stack-walrus.mdx', eval: 'covered' },
      { step: 'Store a blob', stage: 'Store', page: 'sui-stack/walrus/sui-stack-walrus.mdx', eval: 'covered' },
      { step: 'Set blob lifetime', stage: 'Store', page: 'sui-stack/walrus/sui-stack-walrus.mdx', eval: 'covered' },
      // ── Read & Verify ──
      { step: 'Retrieve blob', stage: 'Read & Verify', page: 'sui-stack/walrus/sui-stack-walrus.mdx', eval: 'covered' },
      { step: 'Verify availability & integrity', stage: 'Read & Verify', page: 'sui-stack/walrus/sui-stack-walrus.mdx', eval: 'covered' },
      { step: 'Store sensitive data (Seal)', stage: 'Read & Verify', page: 'sui-stack/seal/sui-stack-seal.mdx', eval: 'covered' },
      // ── Ship ──
      { step: 'Integrate into app', stage: 'Ship', page: 'sui-stack/walrus/sui-stack-walrus.mdx', eval: 'covered' },
      { step: 'Walrus example app (OnlyFins)', stage: 'Ship', page: 'sui-stack/walrus/only-fins.mdx', eval: null },
      { step: 'Walrus custom indexer', stage: 'Ship', page: 'sui-stack/walrus/indexer-walrus.mdx', eval: null },
      { step: 'Custom indexer framework', stage: 'Ship', page: 'develop/accessing-data/custom-indexer/custom-indexers.mdx', eval: null },
      { step: 'Manage lifecycle', stage: 'Ship', page: 'sui-stack/walrus/sui-stack-walrus.mdx', eval: 'covered' },
      { step: 'Dynamic fields for metadata', stage: 'Ship', page: 'develop/objects/dynamic-fields.mdx', eval: null },
      { step: 'Upgrade storage contracts', stage: 'Ship', page: 'develop/publish-upgrade-packages/upgrade.mdx', eval: null },
      { step: 'Security best practices', stage: 'Ship', page: 'develop/security/best-practices.mdx', eval: null },
      { step: 'Harden (errors, retries, cost mgmt)', stage: 'Ship', page: null, eval: 'partial' },
    ],
  },
  {
    id: 'walrus-agentic',
    name: 'Walrus Agentic Data Storage',
    score: 90,
    steps: [
      // ── Environment ──
      { step: 'Programmatic toolchain', stage: 'Environment', page: 'getting-started/onboarding/sui-install.mdx', eval: 'covered' },
      { step: 'Object model for agent blobs', stage: 'Environment', page: 'sui-stack/walrus/sui-stack-walrus.mdx', eval: 'covered' },
      { step: 'Storage resource & payment', stage: 'Environment', page: 'sui-stack/walrus/sui-stack-walrus.mdx', eval: 'covered' },
      { step: 'Object model concepts', stage: 'Environment', page: 'develop/sui-architecture/object-model.mdx', eval: null },
      { step: 'Address-owned objects', stage: 'Environment', page: 'develop/objects/object-ownership/address-owned.mdx', eval: null },
      { step: 'Wrapped objects', stage: 'Environment', page: 'develop/objects/object-ownership/wrapped.mdx', eval: null },
      // ── Agent Write ──
      { step: 'Programmatic store via SDK', stage: 'Agent Write', page: 'sui-stack/walrus/sui-stack-walrus.mdx', eval: 'covered' },
      { step: 'Batch small blobs (Quilt)', stage: 'Agent Write', page: 'sui-stack/walrus/sui-stack-walrus.mdx', eval: 'covered' },
      { step: 'Content addressing & metadata/tags', stage: 'Agent Write', page: 'sui-stack/walrus/sui-stack-walrus.mdx', eval: 'covered' },
      { step: 'Content hashing', stage: 'Agent Write', page: 'develop/cryptography/hashing.mdx', eval: null },
      { step: 'Dynamic fields for versioning', stage: 'Agent Write', page: 'develop/objects/dynamic-fields.mdx', eval: null },
      { step: 'PTB inputs and results', stage: 'Agent Write', page: 'develop/transactions/ptbs/inputs-and-results.mdx', eval: null },
      // ── Verify & Secure ──
      { step: 'Encrypt agent state (Seal)', stage: 'Verify & Secure', page: 'sui-stack/seal/sui-stack-seal.mdx', eval: 'covered' },
      { step: 'Verify availability before acting', stage: 'Verify & Secure', page: 'sui-stack/walrus/sui-stack-walrus.mdx', eval: 'covered' },
      { step: 'Versioned datasets/immutable lineage', stage: 'Verify & Secure', page: 'sui-stack/walrus/sui-stack-walrus.mdx', eval: 'covered' },
      { step: 'Authenticated events', stage: 'Verify & Secure', page: 'develop/accessing-data/authenticated-events.mdx', eval: null },
      { step: 'Event querying', stage: 'Verify & Secure', page: 'develop/accessing-data/using-events.mdx', eval: null },
      // ── Ship ──
      { step: 'Test full agent storage loop', stage: 'Ship', page: null, eval: 'covered' },
      { step: 'Walrus example app (OnlyFins)', stage: 'Ship', page: 'sui-stack/walrus/only-fins.mdx', eval: null },
      { step: 'Walrus custom indexer', stage: 'Ship', page: 'sui-stack/walrus/indexer-walrus.mdx', eval: null },
      { step: 'Build a custom indexer', stage: 'Ship', page: 'develop/accessing-data/custom-indexer/build.mdx', eval: null },
      { step: 'Indexer data integration', stage: 'Ship', page: 'develop/accessing-data/custom-indexer/indexer-data-integration.mdx', eval: null },
      { step: 'Data access interfaces', stage: 'Ship', page: 'develop/accessing-data/data-serving.mdx', eval: null },
      { step: 'GraphQL queries', stage: 'Ship', page: 'develop/accessing-data/graphql/query-with-graphql.mdx', eval: null },
      { step: 'gRPC queries', stage: 'Ship', page: 'develop/accessing-data/grpc/using-grpc.mdx', eval: null },
      { step: 'Upgrade storage contracts', stage: 'Ship', page: 'develop/publish-upgrade-packages/upgrade.mdx', eval: null },
      { step: 'Package versioning', stage: 'Ship', page: 'develop/publish-upgrade-packages/versioning.mdx', eval: null },
      { step: 'Transaction lifecycle', stage: 'Ship', page: 'develop/transactions/transaction-lifecycle.mdx', eval: null },
      { step: 'Security best practices', stage: 'Ship', page: 'develop/security/best-practices.mdx', eval: null },
      { step: 'Automated lifecycle', stage: 'Ship', page: null, eval: 'partial' },
      { step: 'Harden (retries, cost caps, key custody)', stage: 'Ship', page: null, eval: 'partial' },
    ],
  },
];

// ─── Build reverse map: file path → [{pathId, step, stage, eval}] ───────────

function buildPageMap() {
  const map = new Map();
  for (const bp of BUILDER_PATHS) {
    for (const s of bp.steps) {
      if (!s.page) continue;
      if (!map.has(s.page)) map.set(s.page, []);
      map.get(s.page).push({
        path_id: bp.id,
        path_name: bp.name,
        step: s.step,
        stage: s.stage,
        eval: s.eval,
      });
    }
  }
  return map;
}

// ─── Main ───────────────────────────────────────────────────────────────────

function main() {
  const pageMap = buildPageMap();
  let updated = 0;
  let skipped = 0;
  let notFound = 0;

  for (const [relPath, entries] of pageMap) {
    const filePath = path.join(CONTENT_ROOT, relPath);
    if (!fs.existsSync(filePath)) {
      console.error(`WARNING: File not found: ${relPath}`);
      notFound++;
      continue;
    }

    const raw = fs.readFileSync(filePath, 'utf8');
    const { data, content: body } = matter(raw);

    // Build the builder_paths frontmatter — omit eval if null
    data.builder_paths = entries.map(e => {
      const entry = {
        path_id: e.path_id,
        path_name: e.path_name,
        step: e.step,
        stage: e.stage,
      };
      if (e.eval !== null) {
        entry.eval = e.eval;
      }
      return entry;
    });

    // Record where the eval data came from, so the frontmatter is verifiable.
    if (entries.some(e => e.eval !== null)) {
      data.builder_paths_source = {
        dashboard: BUILDER_PATHS_SOURCE.dashboard,
        snapshot: BUILDER_PATHS_SOURCE.snapshotDate,
      };
    }

    if (dryRun) {
      const evalCount = entries.filter(e => e.eval !== null).length;
      const unevalCount = entries.filter(e => e.eval === null).length;
      console.log(`${relPath} (${entries.length} entries: ${evalCount} eval'd, ${unevalCount} uneval'd)`);
    } else {
      const newRaw = matter.stringify(body, data);
      fs.writeFileSync(filePath, newRaw, 'utf8');
    }

    updated++;
  }

  console.log(`${'─'.repeat(50)}`);
  console.log(`${dryRun ? 'DRY RUN' : 'APPLIED'}`);
  console.log(`  Pages updated: ${updated}`);
  console.log(`  Pages not found: ${notFound}`);
  console.log(`  Total unique pages across all paths: ${pageMap.size}`);

  if (dryRun) {
    console.log(`\nRun with --apply to write changes.`);
  }
}

main();
