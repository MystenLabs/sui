import React from 'react';
import ComponentCreator from '@docusaurus/ComponentCreator';

export default [
  {
    path: '/__docusaurus/debug',
    component: ComponentCreator('/__docusaurus/debug', 'd45'),
    exact: true
  },
  {
    path: '/__docusaurus/debug/config',
    component: ComponentCreator('/__docusaurus/debug/config', 'fe0'),
    exact: true
  },
  {
    path: '/__docusaurus/debug/content',
    component: ComponentCreator('/__docusaurus/debug/content', 'b88'),
    exact: true
  },
  {
    path: '/__docusaurus/debug/globalData',
    component: ComponentCreator('/__docusaurus/debug/globalData', 'eb1'),
    exact: true
  },
  {
    path: '/__docusaurus/debug/metadata',
    component: ComponentCreator('/__docusaurus/debug/metadata', '604'),
    exact: true
  },
  {
    path: '/__docusaurus/debug/registry',
    component: ComponentCreator('/__docusaurus/debug/registry', '4cd'),
    exact: true
  },
  {
    path: '/__docusaurus/debug/routes',
    component: ComponentCreator('/__docusaurus/debug/routes', 'f0c'),
    exact: true
  },
  {
    path: '/sui-api-ref',
    component: ComponentCreator('/sui-api-ref', 'd1a'),
    exact: true
  },
  {
    path: '/',
    component: ComponentCreator('/', 'ca3'),
    exact: true
  },
  {
    path: '/',
    component: ComponentCreator('/', '83a'),
    routes: [
      {
        path: '/bridging',
        component: ComponentCreator('/bridging', '996'),
        exact: true
      },
      {
        path: '/code-of-conduct',
        component: ComponentCreator('/code-of-conduct', '2ac'),
        exact: true,
        sidebar: "referencesSidebar"
      },
      {
        path: '/concepts',
        component: ComponentCreator('/concepts', '7fe'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/app-devs',
        component: ComponentCreator('/concepts/app-devs', 'c43'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/architecture',
        component: ComponentCreator('/concepts/architecture', 'aef'),
        exact: true
      },
      {
        path: '/concepts/components',
        component: ComponentCreator('/concepts/components', '782'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/cryptography',
        component: ComponentCreator('/concepts/cryptography', 'bcc'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/cryptography/smart-contracts',
        component: ComponentCreator('/concepts/cryptography/smart-contracts', '2a8'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/cryptography/smart-contracts/ecvrf',
        component: ComponentCreator('/concepts/cryptography/smart-contracts/ecvrf', '501'),
        exact: true
      },
      {
        path: '/concepts/cryptography/smart-contracts/groth16',
        component: ComponentCreator('/concepts/cryptography/smart-contracts/groth16', '270'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/cryptography/smart-contracts/hashing',
        component: ComponentCreator('/concepts/cryptography/smart-contracts/hashing', '10c'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/cryptography/smart-contracts/signing',
        component: ComponentCreator('/concepts/cryptography/smart-contracts/signing', '2dd'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/cryptography/system',
        component: ComponentCreator('/concepts/cryptography/system', 'b00'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/cryptography/system/checkpoint-verification',
        component: ComponentCreator('/concepts/cryptography/system/checkpoint-verification', '385'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/cryptography/system/intents-for-validation',
        component: ComponentCreator('/concepts/cryptography/system/intents-for-validation', '30d'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/cryptography/system/validator-signatures',
        component: ComponentCreator('/concepts/cryptography/system/validator-signatures', '2d6'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/cryptography/transaction-auth',
        component: ComponentCreator('/concepts/cryptography/transaction-auth', 'ae1'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/cryptography/transaction-auth/intent-signing',
        component: ComponentCreator('/concepts/cryptography/transaction-auth/intent-signing', '343'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/cryptography/transaction-auth/keys-addresses',
        component: ComponentCreator('/concepts/cryptography/transaction-auth/keys-addresses', '550'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/cryptography/transaction-auth/multisig',
        component: ComponentCreator('/concepts/cryptography/transaction-auth/multisig', '554'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/cryptography/transaction-auth/offline-signing',
        component: ComponentCreator('/concepts/cryptography/transaction-auth/offline-signing', 'f3c'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/cryptography/transaction-auth/signatures',
        component: ComponentCreator('/concepts/cryptography/transaction-auth/signatures', 'a61'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/cryptography/zklogin',
        component: ComponentCreator('/concepts/cryptography/zklogin', '490'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/dynamic-fields',
        component: ComponentCreator('/concepts/dynamic-fields', '20f'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/dynamic-fields/dynamic-object-fields',
        component: ComponentCreator('/concepts/dynamic-fields/dynamic-object-fields', '886'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/dynamic-fields/events',
        component: ComponentCreator('/concepts/dynamic-fields/events', 'a70'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/dynamic-fields/tables-bags',
        component: ComponentCreator('/concepts/dynamic-fields/tables-bags', '264'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/dynamic-fields/transfers',
        component: ComponentCreator('/concepts/dynamic-fields/transfers', '6bd'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/dynamic-fields/transfers/custom-rules',
        component: ComponentCreator('/concepts/dynamic-fields/transfers/custom-rules', '211'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/dynamic-fields/transfers/transfer-to-object',
        component: ComponentCreator('/concepts/dynamic-fields/transfers/transfer-to-object', '03b'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/dynamic-fields/versioning',
        component: ComponentCreator('/concepts/dynamic-fields/versioning', '85c'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/object-model',
        component: ComponentCreator('/concepts/object-model', 'efb'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/object-ownership',
        component: ComponentCreator('/concepts/object-ownership', 'df7'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/object-ownership/address-owned',
        component: ComponentCreator('/concepts/object-ownership/address-owned', '9cf'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/object-ownership/immutable',
        component: ComponentCreator('/concepts/object-ownership/immutable', '9c5'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/object-ownership/shared',
        component: ComponentCreator('/concepts/object-ownership/shared', 'ada'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/object-ownership/wrapped',
        component: ComponentCreator('/concepts/object-ownership/wrapped', 'ddd'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/sui-architecture',
        component: ComponentCreator('/concepts/sui-architecture', '2dc'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/sui-architecture/certification-overview',
        component: ComponentCreator('/concepts/sui-architecture/certification-overview', '4aa'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/sui-architecture/confirmation',
        component: ComponentCreator('/concepts/sui-architecture/confirmation', 'b20'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/sui-architecture/consensus',
        component: ComponentCreator('/concepts/sui-architecture/consensus', 'af2'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/sui-architecture/data-management-things',
        component: ComponentCreator('/concepts/sui-architecture/data-management-things', '429'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/sui-architecture/epochs',
        component: ComponentCreator('/concepts/sui-architecture/epochs', '67e'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/sui-architecture/high-level',
        component: ComponentCreator('/concepts/sui-architecture/high-level', 'ac0'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/sui-architecture/indexer-functions',
        component: ComponentCreator('/concepts/sui-architecture/indexer-functions', 'a9d'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/sui-architecture/protocol-upgrades',
        component: ComponentCreator('/concepts/sui-architecture/protocol-upgrades', '1d0'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/sui-architecture/transaction-lifetime',
        component: ComponentCreator('/concepts/sui-architecture/transaction-lifetime', '313'),
        exact: true
      },
      {
        path: '/concepts/sui-move-concepts',
        component: ComponentCreator('/concepts/sui-move-concepts', 'b98'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/sui-move-concepts/collections',
        component: ComponentCreator('/concepts/sui-move-concepts/collections', 'c9c'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/sui-move-concepts/entry-functions',
        component: ComponentCreator('/concepts/sui-move-concepts/entry-functions', '114'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/sui-move-concepts/init',
        component: ComponentCreator('/concepts/sui-move-concepts/init', 'e92'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/sui-move-concepts/language',
        component: ComponentCreator('/concepts/sui-move-concepts/language', '27b'),
        exact: true
      },
      {
        path: '/concepts/sui-move-concepts/move-on-sui',
        component: ComponentCreator('/concepts/sui-move-concepts/move-on-sui', 'c1d'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/sui-move-concepts/one-time-witness',
        component: ComponentCreator('/concepts/sui-move-concepts/one-time-witness', '217'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/sui-move-concepts/packages',
        component: ComponentCreator('/concepts/sui-move-concepts/packages', '1de'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/sui-move-concepts/packages/custom-policies',
        component: ComponentCreator('/concepts/sui-move-concepts/packages/custom-policies', 'e45'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/sui-move-concepts/packages/upgrade',
        component: ComponentCreator('/concepts/sui-move-concepts/packages/upgrade', 'cc7'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/sui-move-concepts/patterns',
        component: ComponentCreator('/concepts/sui-move-concepts/patterns', '241'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/sui-move-concepts/patterns/app-extensions',
        component: ComponentCreator('/concepts/sui-move-concepts/patterns/app-extensions', 'cff'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/sui-move-concepts/patterns/capabilities',
        component: ComponentCreator('/concepts/sui-move-concepts/patterns/capabilities', '2b1'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/sui-move-concepts/patterns/hot-potato',
        component: ComponentCreator('/concepts/sui-move-concepts/patterns/hot-potato', '7f9'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/sui-move-concepts/patterns/id-pointer',
        component: ComponentCreator('/concepts/sui-move-concepts/patterns/id-pointer', 'ea9'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/sui-move-concepts/patterns/transferrable-witness',
        component: ComponentCreator('/concepts/sui-move-concepts/patterns/transferrable-witness', 'd48'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/sui-move-concepts/patterns/witness',
        component: ComponentCreator('/concepts/sui-move-concepts/patterns/witness', '1f2'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/sui-move-concepts/strings',
        component: ComponentCreator('/concepts/sui-move-concepts/strings', '380'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/tokenomics',
        component: ComponentCreator('/concepts/tokenomics', '2b7'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/tokenomics/gas-in-sui',
        component: ComponentCreator('/concepts/tokenomics/gas-in-sui', '3aa'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/tokenomics/gas-pricing',
        component: ComponentCreator('/concepts/tokenomics/gas-pricing', 'dfc'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/tokenomics/proof-of-stake',
        component: ComponentCreator('/concepts/tokenomics/proof-of-stake', '6b7'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/tokenomics/staking-unstaking',
        component: ComponentCreator('/concepts/tokenomics/staking-unstaking', '517'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/tokenomics/storage-fund',
        component: ComponentCreator('/concepts/tokenomics/storage-fund', 'b53'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/tokenomics/sui-token',
        component: ComponentCreator('/concepts/tokenomics/sui-token', '1e7'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/tokenomics/validators-staking',
        component: ComponentCreator('/concepts/tokenomics/validators-staking', '988'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/transactions',
        component: ComponentCreator('/concepts/transactions', '368'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/transactions/certification-overview',
        component: ComponentCreator('/concepts/transactions/certification-overview', '73c'),
        exact: true
      },
      {
        path: '/concepts/transactions/gas-smashing',
        component: ComponentCreator('/concepts/transactions/gas-smashing', '097'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/transactions/input-types',
        component: ComponentCreator('/concepts/transactions/input-types', '9cd'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/transactions/prog-txn-blocks',
        component: ComponentCreator('/concepts/transactions/prog-txn-blocks', '06c'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/transactions/result-and-nested',
        component: ComponentCreator('/concepts/transactions/result-and-nested', '4f9'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/transactions/sponsored-transactions',
        component: ComponentCreator('/concepts/transactions/sponsored-transactions', 'dac'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/transactions/transaction-lifecycle',
        component: ComponentCreator('/concepts/transactions/transaction-lifecycle', '6e5'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/transactions/transaction-things',
        component: ComponentCreator('/concepts/transactions/transaction-things', '5ea'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/transactions/transaction-types',
        component: ComponentCreator('/concepts/transactions/transaction-types', '00b'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/transactions/transaction-types/merge-coin',
        component: ComponentCreator('/concepts/transactions/transaction-types/merge-coin', 'bac'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/transactions/transaction-types/move-call',
        component: ComponentCreator('/concepts/transactions/transaction-types/move-call', '6da'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/transactions/transaction-types/split-coin',
        component: ComponentCreator('/concepts/transactions/transaction-types/split-coin', '300'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/concepts/transactions/transaction-types/transfer-object',
        component: ComponentCreator('/concepts/transactions/transaction-types/transfer-object', '901'),
        exact: true,
        sidebar: "conceptsSidebar"
      },
      {
        path: '/connect-button',
        component: ComponentCreator('/connect-button', '3b4'),
        exact: true
      },
      {
        path: '/contribute-to-sui-repos',
        component: ComponentCreator('/contribute-to-sui-repos', 'c37'),
        exact: true,
        sidebar: "referencesSidebar"
      },
      {
        path: '/deepbook-design',
        component: ComponentCreator('/deepbook-design', '7a2'),
        exact: true,
        sidebar: "standardsSidebar"
      },
      {
        path: '/deepbook-orders',
        component: ComponentCreator('/deepbook-orders', 'a6e'),
        exact: true,
        sidebar: "standardsSidebar"
      },
      {
        path: '/deepbook-pools',
        component: ComponentCreator('/deepbook-pools', '288'),
        exact: true,
        sidebar: "standardsSidebar"
      },
      {
        path: '/examples',
        component: ComponentCreator('/examples', 'be9'),
        exact: true
      },
      {
        path: '/exchange-integration-guide',
        component: ComponentCreator('/exchange-integration-guide', '139'),
        exact: true
      },
      {
        path: '/guides',
        component: ComponentCreator('/guides', '784'),
        exact: true,
        sidebar: "guidesSidebar"
      },
      {
        path: '/guides/developer',
        component: ComponentCreator('/guides/developer', '952'),
        exact: true,
        sidebar: "guidesSidebar"
      },
      {
        path: '/guides/developer/advanced',
        component: ComponentCreator('/guides/developer/advanced', 'e9c'),
        exact: true,
        sidebar: "guidesSidebar"
      },
      {
        path: '/guides/developer/advanced/efficient-smart-contracts',
        component: ComponentCreator('/guides/developer/advanced/efficient-smart-contracts', 'ab8'),
        exact: true,
        sidebar: "guidesSidebar"
      },
      {
        path: '/guides/developer/advanced/maximize-reach',
        component: ComponentCreator('/guides/developer/advanced/maximize-reach', '804'),
        exact: true,
        sidebar: "guidesSidebar"
      },
      {
        path: '/guides/developer/advanced/min-gas-fees',
        component: ComponentCreator('/guides/developer/advanced/min-gas-fees', 'bce'),
        exact: true,
        sidebar: "guidesSidebar"
      },
      {
        path: '/guides/developer/advanced/security-best-practices',
        component: ComponentCreator('/guides/developer/advanced/security-best-practices', '5f4'),
        exact: true,
        sidebar: "guidesSidebar"
      },
      {
        path: '/guides/developer/advanced/wallet-integrations',
        component: ComponentCreator('/guides/developer/advanced/wallet-integrations', '1ce'),
        exact: true,
        sidebar: "guidesSidebar"
      },
      {
        path: '/guides/developer/app-examples',
        component: ComponentCreator('/guides/developer/app-examples', 'dbc'),
        exact: true,
        sidebar: "guidesSidebar"
      },
      {
        path: '/guides/developer/app-examples/auction',
        component: ComponentCreator('/guides/developer/app-examples/auction', '91b'),
        exact: true,
        sidebar: "guidesSidebar"
      },
      {
        path: '/guides/developer/app-examples/blackjack',
        component: ComponentCreator('/guides/developer/app-examples/blackjack', '1cd'),
        exact: true,
        sidebar: "guidesSidebar"
      },
      {
        path: '/guides/developer/app-examples/coin-flip',
        component: ComponentCreator('/guides/developer/app-examples/coin-flip', 'f8e'),
        exact: true,
        sidebar: "guidesSidebar"
      },
      {
        path: '/guides/developer/app-examples/escrow',
        component: ComponentCreator('/guides/developer/app-examples/escrow', '7ce'),
        exact: true,
        sidebar: "guidesSidebar"
      },
      {
        path: '/guides/developer/app-examples/meta-pricing-oracle',
        component: ComponentCreator('/guides/developer/app-examples/meta-pricing-oracle', '9ab'),
        exact: true,
        sidebar: "guidesSidebar"
      },
      {
        path: '/guides/developer/app-examples/oracle',
        component: ComponentCreator('/guides/developer/app-examples/oracle', 'cf3'),
        exact: true,
        sidebar: "guidesSidebar"
      },
      {
        path: '/guides/developer/app-examples/tic-tac-toe',
        component: ComponentCreator('/guides/developer/app-examples/tic-tac-toe', '542'),
        exact: true,
        sidebar: "guidesSidebar"
      },
      {
        path: '/guides/developer/app-examples/trusted-swap',
        component: ComponentCreator('/guides/developer/app-examples/trusted-swap', '176'),
        exact: true,
        sidebar: "guidesSidebar"
      },
      {
        path: '/guides/developer/app-examples/turnip-town',
        component: ComponentCreator('/guides/developer/app-examples/turnip-town', '7bd'),
        exact: true,
        sidebar: "guidesSidebar"
      },
      {
        path: '/guides/developer/app-examples/weather-oracle',
        component: ComponentCreator('/guides/developer/app-examples/weather-oracle', 'dbf'),
        exact: true,
        sidebar: "guidesSidebar"
      },
      {
        path: '/guides/developer/dev-cheat-sheet',
        component: ComponentCreator('/guides/developer/dev-cheat-sheet', '529'),
        exact: true,
        sidebar: "guidesSidebar"
      },
      {
        path: '/guides/developer/examples',
        component: ComponentCreator('/guides/developer/examples', '2bc'),
        exact: true
      },
      {
        path: '/guides/developer/first-app',
        component: ComponentCreator('/guides/developer/first-app', '9a2'),
        exact: true,
        sidebar: "guidesSidebar"
      },
      {
        path: '/guides/developer/first-app/build-test',
        component: ComponentCreator('/guides/developer/first-app/build-test', '81b'),
        exact: true,
        sidebar: "guidesSidebar"
      },
      {
        path: '/guides/developer/first-app/client-tssdk',
        component: ComponentCreator('/guides/developer/first-app/client-tssdk', 'd47'),
        exact: true,
        sidebar: "guidesSidebar"
      },
      {
        path: '/guides/developer/first-app/debug',
        component: ComponentCreator('/guides/developer/first-app/debug', '204'),
        exact: true,
        sidebar: "guidesSidebar"
      },
      {
        path: '/guides/developer/first-app/publish',
        component: ComponentCreator('/guides/developer/first-app/publish', '84f'),
        exact: true,
        sidebar: "guidesSidebar"
      },
      {
        path: '/guides/developer/first-app/write-package',
        component: ComponentCreator('/guides/developer/first-app/write-package', 'c49'),
        exact: true
      },
      {
        path: '/guides/developer/getting-started',
        component: ComponentCreator('/guides/developer/getting-started', 'a33'),
        exact: true,
        sidebar: "guidesSidebar"
      },
      {
        path: '/guides/developer/getting-started/connect',
        component: ComponentCreator('/guides/developer/getting-started/connect', '088'),
        exact: true,
        sidebar: "guidesSidebar"
      },
      {
        path: '/guides/developer/getting-started/get-address',
        component: ComponentCreator('/guides/developer/getting-started/get-address', '996'),
        exact: true,
        sidebar: "guidesSidebar"
      },
      {
        path: '/guides/developer/getting-started/get-coins',
        component: ComponentCreator('/guides/developer/getting-started/get-coins', '489'),
        exact: true,
        sidebar: "guidesSidebar"
      },
      {
        path: '/guides/developer/getting-started/local-network',
        component: ComponentCreator('/guides/developer/getting-started/local-network', 'c46'),
        exact: true,
        sidebar: "guidesSidebar"
      },
      {
        path: '/guides/developer/getting-started/sui-environment',
        component: ComponentCreator('/guides/developer/getting-started/sui-environment', '278'),
        exact: true,
        sidebar: "guidesSidebar"
      },
      {
        path: '/guides/developer/getting-started/sui-install',
        component: ComponentCreator('/guides/developer/getting-started/sui-install', '57e'),
        exact: true,
        sidebar: "guidesSidebar"
      },
      {
        path: '/guides/developer/starter-templates',
        component: ComponentCreator('/guides/developer/starter-templates', 'bc8'),
        exact: true,
        sidebar: "guidesSidebar"
      },
      {
        path: '/guides/developer/sui-101',
        component: ComponentCreator('/guides/developer/sui-101', 'ca9'),
        exact: true,
        sidebar: "guidesSidebar"
      },
      {
        path: '/guides/developer/sui-101/access-time',
        component: ComponentCreator('/guides/developer/sui-101/access-time', '9c6'),
        exact: true,
        sidebar: "guidesSidebar"
      },
      {
        path: '/guides/developer/sui-101/building-ptb',
        component: ComponentCreator('/guides/developer/sui-101/building-ptb', '487'),
        exact: true,
        sidebar: "guidesSidebar"
      },
      {
        path: '/guides/developer/sui-101/coin-mgt',
        component: ComponentCreator('/guides/developer/sui-101/coin-mgt', '20b'),
        exact: true,
        sidebar: "guidesSidebar"
      },
      {
        path: '/guides/developer/sui-101/create-coin',
        component: ComponentCreator('/guides/developer/sui-101/create-coin', 'be8'),
        exact: true,
        sidebar: "guidesSidebar"
      },
      {
        path: '/guides/developer/sui-101/create-nft',
        component: ComponentCreator('/guides/developer/sui-101/create-nft', '6bb'),
        exact: true,
        sidebar: "guidesSidebar"
      },
      {
        path: '/guides/developer/sui-101/send-txn',
        component: ComponentCreator('/guides/developer/sui-101/send-txn', 'ec9'),
        exact: true,
        sidebar: "guidesSidebar"
      },
      {
        path: '/guides/developer/sui-101/shared-owned',
        component: ComponentCreator('/guides/developer/sui-101/shared-owned', '095'),
        exact: true,
        sidebar: "guidesSidebar"
      },
      {
        path: '/guides/developer/sui-101/sign-txn',
        component: ComponentCreator('/guides/developer/sui-101/sign-txn', '4fd'),
        exact: true,
        sidebar: "guidesSidebar"
      },
      {
        path: '/guides/developer/sui-101/simulating-refs',
        component: ComponentCreator('/guides/developer/sui-101/simulating-refs', 'add'),
        exact: true,
        sidebar: "guidesSidebar"
      },
      {
        path: '/guides/developer/sui-101/sponsor-txn',
        component: ComponentCreator('/guides/developer/sui-101/sponsor-txn', 'ad3'),
        exact: true,
        sidebar: "guidesSidebar"
      },
      {
        path: '/guides/developer/sui-101/using-events',
        component: ComponentCreator('/guides/developer/sui-101/using-events', '507'),
        exact: true,
        sidebar: "guidesSidebar"
      },
      {
        path: '/guides/developer/sui-101/working-with-ptbs',
        component: ComponentCreator('/guides/developer/sui-101/working-with-ptbs', '1f2'),
        exact: true,
        sidebar: "guidesSidebar"
      },
      {
        path: '/guides/developer/zklogin-onboarding',
        component: ComponentCreator('/guides/developer/zklogin-onboarding', '088'),
        exact: true,
        sidebar: "guidesSidebar"
      },
      {
        path: '/guides/operator',
        component: ComponentCreator('/guides/operator', 'a73'),
        exact: true,
        sidebar: "guidesSidebar"
      },
      {
        path: '/guides/operator/archives',
        component: ComponentCreator('/guides/operator/archives', '364'),
        exact: true,
        sidebar: "guidesSidebar"
      },
      {
        path: '/guides/operator/data-management',
        component: ComponentCreator('/guides/operator/data-management', '94c'),
        exact: true,
        sidebar: "guidesSidebar"
      },
      {
        path: '/guides/operator/genesis',
        component: ComponentCreator('/guides/operator/genesis', 'ea3'),
        exact: true,
        sidebar: "guidesSidebar"
      },
      {
        path: '/guides/operator/node-tools',
        component: ComponentCreator('/guides/operator/node-tools', '953'),
        exact: true,
        sidebar: "guidesSidebar"
      },
      {
        path: '/guides/operator/observability',
        component: ComponentCreator('/guides/operator/observability', 'd07'),
        exact: true
      },
      {
        path: '/guides/operator/snapshots',
        component: ComponentCreator('/guides/operator/snapshots', '204'),
        exact: true,
        sidebar: "guidesSidebar"
      },
      {
        path: '/guides/operator/staking-rewards',
        component: ComponentCreator('/guides/operator/staking-rewards', 'ac1'),
        exact: true,
        sidebar: "guidesSidebar"
      },
      {
        path: '/guides/operator/sui-full-node',
        component: ComponentCreator('/guides/operator/sui-full-node', 'f54'),
        exact: true,
        sidebar: "guidesSidebar"
      },
      {
        path: '/guides/operator/validator-committee',
        component: ComponentCreator('/guides/operator/validator-committee', 'ee9'),
        exact: true,
        sidebar: "guidesSidebar"
      },
      {
        path: '/guides/operator/validator-config',
        component: ComponentCreator('/guides/operator/validator-config', 'c66'),
        exact: true,
        sidebar: "guidesSidebar"
      },
      {
        path: '/guides/operator/validator-tasks',
        component: ComponentCreator('/guides/operator/validator-tasks', '7e2'),
        exact: true,
        sidebar: "guidesSidebar"
      },
      {
        path: '/localize-sui-docs',
        component: ComponentCreator('/localize-sui-docs', 'cca'),
        exact: true,
        sidebar: "referencesSidebar"
      },
      {
        path: '/programmatic-connection',
        component: ComponentCreator('/programmatic-connection', '6bc'),
        exact: true
      },
      {
        path: '/query-the-pool',
        component: ComponentCreator('/query-the-pool', '889'),
        exact: true,
        sidebar: "standardsSidebar"
      },
      {
        path: '/references',
        component: ComponentCreator('/references', '9b2'),
        exact: true,
        sidebar: "referencesSidebar"
      },
      {
        path: '/references/cli',
        component: ComponentCreator('/references/cli', '250'),
        exact: true,
        sidebar: "referencesSidebar"
      },
      {
        path: '/references/cli/client',
        component: ComponentCreator('/references/cli/client', 'd89'),
        exact: true,
        sidebar: "referencesSidebar"
      },
      {
        path: '/references/cli/console',
        component: ComponentCreator('/references/cli/console', 'bac'),
        exact: true,
        sidebar: "referencesSidebar"
      },
      {
        path: '/references/cli/keytool',
        component: ComponentCreator('/references/cli/keytool', '37c'),
        exact: true,
        sidebar: "referencesSidebar"
      },
      {
        path: '/references/cli/move',
        component: ComponentCreator('/references/cli/move', '86a'),
        exact: true,
        sidebar: "referencesSidebar"
      },
      {
        path: '/references/cli/validator',
        component: ComponentCreator('/references/cli/validator', '9f5'),
        exact: true,
        sidebar: "referencesSidebar"
      },
      {
        path: '/references/contribute/contribution-process',
        component: ComponentCreator('/references/contribute/contribution-process', 'dc9'),
        exact: true,
        sidebar: "referencesSidebar"
      },
      {
        path: '/references/dapp-kit',
        component: ComponentCreator('/references/dapp-kit', '41d'),
        exact: true
      },
      {
        path: '/references/event-query-and-subscription',
        component: ComponentCreator('/references/event-query-and-subscription', '9d5'),
        exact: true
      },
      {
        path: '/references/move/language',
        component: ComponentCreator('/references/move/language', 'c12'),
        exact: true
      },
      {
        path: '/references/move/move-lock',
        component: ComponentCreator('/references/move/move-lock', 'e82'),
        exact: true,
        sidebar: "referencesSidebar"
      },
      {
        path: '/references/move/move-toml',
        component: ComponentCreator('/references/move/move-toml', 'b56'),
        exact: true,
        sidebar: "referencesSidebar"
      },
      {
        path: '/references/sdk/rust-sdk',
        component: ComponentCreator('/references/sdk/rust-sdk', '50c'),
        exact: true,
        sidebar: "referencesSidebar"
      },
      {
        path: '/references/sui-api',
        component: ComponentCreator('/references/sui-api', 'd5e'),
        exact: true,
        sidebar: "referencesSidebar"
      },
      {
        path: '/references/sui-api/json-rpc-format',
        component: ComponentCreator('/references/sui-api/json-rpc-format', 'fc0'),
        exact: true,
        sidebar: "referencesSidebar"
      },
      {
        path: '/references/sui-api/rpc-api',
        component: ComponentCreator('/references/sui-api/rpc-api', '960'),
        exact: true
      },
      {
        path: '/references/sui-api/rpc-best-practices',
        component: ComponentCreator('/references/sui-api/rpc-best-practices', '36d'),
        exact: true,
        sidebar: "referencesSidebar"
      },
      {
        path: '/references/sui-move',
        component: ComponentCreator('/references/sui-move', '7e2'),
        exact: true,
        sidebar: "referencesSidebar"
      },
      {
        path: '/references/sui-sdks',
        component: ComponentCreator('/references/sui-sdks', '6ef'),
        exact: true,
        sidebar: "referencesSidebar"
      },
      {
        path: '/research',
        component: ComponentCreator('/research', '7ed'),
        exact: true
      },
      {
        path: '/routing-a-swap',
        component: ComponentCreator('/routing-a-swap', 'be5'),
        exact: true,
        sidebar: "standardsSidebar"
      },
      {
        path: '/snippets/staking-pool-reqs',
        component: ComponentCreator('/snippets/staking-pool-reqs', '661'),
        exact: true
      },
      {
        path: '/standards',
        component: ComponentCreator('/standards', 'e33'),
        exact: true,
        sidebar: "standardsSidebar"
      },
      {
        path: '/standards/deepbook',
        component: ComponentCreator('/standards/deepbook', '97e'),
        exact: true,
        sidebar: "standardsSidebar"
      },
      {
        path: '/standards/display',
        component: ComponentCreator('/standards/display', 'b6c'),
        exact: true,
        sidebar: "standardsSidebar"
      },
      {
        path: '/standards/kiosk',
        component: ComponentCreator('/standards/kiosk', 'e72'),
        exact: true,
        sidebar: "standardsSidebar"
      },
      {
        path: '/standards/wallet-adapter',
        component: ComponentCreator('/standards/wallet-adapter', '230'),
        exact: true,
        sidebar: "standardsSidebar"
      },
      {
        path: '/style-guide',
        component: ComponentCreator('/style-guide', '5c7'),
        exact: true,
        sidebar: "referencesSidebar"
      },
      {
        path: '/sui-compared',
        component: ComponentCreator('/sui-compared', 'ab5'),
        exact: true
      },
      {
        path: '/sui-framework-reference',
        component: ComponentCreator('/sui-framework-reference', 'ec9'),
        exact: true
      },
      {
        path: '/sui-glossary',
        component: ComponentCreator('/sui-glossary', '799'),
        exact: true
      },
      {
        path: '/sui-security',
        component: ComponentCreator('/sui-security', '189'),
        exact: true
      },
      {
        path: '/trade-and-swap',
        component: ComponentCreator('/trade-and-swap', '3d2'),
        exact: true,
        sidebar: "standardsSidebar"
      },
      {
        path: '/use-wallet-kit',
        component: ComponentCreator('/use-wallet-kit', '0a3'),
        exact: true
      },
      {
        path: '/wallet-adapters',
        component: ComponentCreator('/wallet-adapters', '31b'),
        exact: true
      },
      {
        path: '/wallet-kit-core',
        component: ComponentCreator('/wallet-kit-core', '105'),
        exact: true
      },
      {
        path: '/wallet-kit-getting-started',
        component: ComponentCreator('/wallet-kit-getting-started', '807'),
        exact: true
      },
      {
        path: '/wallet-kit-introduction',
        component: ComponentCreator('/wallet-kit-introduction', 'cd6'),
        exact: true
      },
      {
        path: '/wallet-kit-provider',
        component: ComponentCreator('/wallet-kit-provider', '05f'),
        exact: true
      },
      {
        path: '/wallet-standard',
        component: ComponentCreator('/wallet-standard', '23a'),
        exact: true
      }
    ]
  },
  {
    path: '*',
    component: ComponentCreator('*'),
  },
];
