// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const guides = [
  {
    type: 'doc',
    label: 'Developer Guides',
    id: 'guides',
  },
  {
    type: 'category',
    label: 'Getting Started',
    collapsed: false,
    link: {
      type: 'doc',
      id: 'guides/developer/getting-started/index',
    },
    items: [
      {
        type: 'category',
        label: 'Install Sui',
        collapsed: false,
        link: {
          type: 'doc',
          id: 'guides/developer/getting-started/sui-install',
        },
        items: [
          'guides/developer/getting-started/install-source',
          'guides/developer/getting-started/install-binaries',
          'guides/developer/getting-started/local-network',
        ],
      },
      'guides/developer/getting-started/configure-sui-client',
      'guides/developer/getting-started/get-address',
      'guides/developer/getting-started/get-coins',
      'guides/developer/getting-started/hello-world',
      'guides/developer/getting-started/app-frontends',
      'guides/developer/getting-started/next-steps',
    ],
  },
  {
    type: 'category',
    label: 'Objects',
    link: {
      type: 'doc',
      id: 'guides/developer/objects/index',
    },
    items: [
      'guides/developer/objects/object-model',
      {
        type: 'category',
        label: 'Types of Object Ownership',
        link: {
          type: 'doc',
          id: 'guides/developer/objects/object-ownership/index',
        },
        items: [
          'guides/developer/objects/object-ownership/address-owned',
          'guides/developer/objects/object-ownership/immutable',
          'guides/developer/objects/object-ownership/party',
          'guides/developer/objects/object-ownership/shared',
          'guides/developer/objects/object-ownership/wrapped',
        ],
      },
      {
        type: 'category',
        label: 'Transfering Objects',
        link: {
          type: 'doc',
          id: 'guides/developer/objects/transfers/index',
        },
        items: [
          'guides/developer/objects/transfers/custom-rules',
          'guides/developer/objects/transfers/transfer-policies',
          'guides/developer/objects/transfers/transfer-to-object',
        ],
      },
      {
        type: 'category',
        label: 'Object Display',
        link: {
          type: 'doc',
          id: 'guides/developer/objects/display/index',
        },
        items: [
          'guides/developer/objects/display/display-overview',
          'guides/developer/objects/display/using-display',
          'guides/developer/objects/display/display-preview'
        ],
      },
      'guides/developer/objects/derived-objects',
      {
        type: 'category',
        label: 'Dynamic Fields',
        link: {
          type: 'doc',
          id: 'guides/developer/objects/dynamic-fields',
        },
        items: ['guides/developer/objects/tables-bags'],
      },
      'guides/developer/objects/versioning',
      'guides/developer/objects/local-fee-markets',
      'guides/developer/objects/simulating-refs',
    ],
  },
  {
    type: 'category',
    label: 'Packages',
    link: {
      type: 'doc',
      id: 'guides/developer/packages/index',
        },
    items: [
      'guides/developer/packages/package-overview',
      'guides/developer/packages/move-package-management',
      'guides/developer/packages/upgrade',
      'guides/developer/packages/custom-policies',
      'guides/developer/packages/automated-address-management',
	  'guides/developer/packages/openzeppelin',
    ],
  },
  {
    type: 'category',
    label: 'Transactions',
    link: {
      type: 'doc',
      id: 'guides/developer/transactions/index',
    },
    items: [
      'guides/developer/transactions/txn-overview',
      'guides/developer/transactions/transaction-lifecycle',
      {
        type: 'category',
        label: 'Programmable Transaction Blocks (PTBs)',
        link: {
          type: 'doc',
          id: 'guides/developer/transactions/ptbs/index',
        },
        items: [
          'guides/developer/transactions/ptbs/prog-txn-blocks',
          'guides/developer/transactions/ptbs/building-ptb',
          'guides/developer/transactions/ptbs/inputs-and-results',
          'guides/developer/transactions/ptbs/sign-and-send-txn',
        ],
      },
      {
        type: 'category',
        label: 'Transaction Authentication',
        link: {
          type: 'doc',
          id: 'guides/developer/transactions/transaction-auth/index',
        },
        items: [
          'guides/developer/transactions/transaction-auth/intent-signing',
          'guides/developer/transactions/transaction-auth/multisig',
          'guides/developer/transactions/transaction-auth/offline-signing',
		  'guides/developer/transactions/transaction-auth/address-aliases',
        ],
      },
      'guides/developer/transactions/sponsor-txn',
			'guides/developer/transactions/gas-smashing',
    ],
  },
  {
    type: 'category',
    label: 'Accessing Data',
     link: {
          type: 'doc',
          id: 'guides/developer/accessing-data/index',
        },
    items: [
      'guides/developer/accessing-data/grpc-overview',
      'guides/developer/accessing-data/query-with-graphql',
      'guides/developer/accessing-data/archival-store',
      'guides/developer/accessing-data/using-events',
      {
        type: 'category',
        label: 'Custom Indexing Framework',
        link: {
          type: 'doc',
          id: 'guides/developer/accessing-data/custom-indexer/index',
        },
        items: [
          'guides/developer/accessing-data/custom-indexer/build',
          'guides/developer/accessing-data/custom-indexer/indexer-walrus',
          'guides/developer/accessing-data/custom-indexer/bring-your-own-store',
        ],
      },
    ],
  },
  {
    type: 'category',
    label: 'Digital Assets',
    link: {
      type: 'doc',
      id: 'guides/developer/digital-assets/index',
    },
    items: [
      'guides/developer/digital-assets/types-of-assets',
      {
        type: 'category',
        label: 'Fungible Tokens',
        link: {
          type: 'doc',
          id: 'guides/developer/digital-assets/fungible-tokens/index',
        },
        items: [
          'guides/developer/digital-assets/fungible-tokens/create-a-fungible-token-coin',
          'guides/developer/digital-assets/fungible-tokens/create-a-fungible-token',
          'guides/developer/digital-assets/fungible-tokens/regulated-tokens',
          'guides/developer/digital-assets/fungible-tokens/token-vesting-strategies',
        ],
      },
      {
        type: 'category',
        label: 'Tokenized Assets',
        link: {
          type: 'doc',
          id: 'guides/developer/digital-assets/non-fungible-tokens/index',
        },
        items: [
          'guides/developer/digital-assets/non-fungible-tokens/asset-tokenization',
          'guides/developer/digital-assets/non-fungible-tokens/deploy-tokenized-asset',
          'guides/developer/digital-assets/non-fungible-tokens/create-nft',
        ],
      },
      'guides/developer/digital-assets/permissioned-assets',
      'guides/developer/digital-assets/gasless-transactions',
      'guides/developer/digital-assets/migrate-address-balances',
      {
        type: 'category',
        label: 'Examples and Patterns',
        link: {
          type: 'doc',
          id: 'guides/developer/digital-assets/examples-patterns/index',
        },
        items: [
          'guides/developer/digital-assets/examples-patterns/in-game-currency',
          'guides/developer/digital-assets/examples-patterns/loyalty-tokens',
          'guides/developer/digital-assets/examples-patterns/fixed-supply',
          'guides/developer/digital-assets/examples-patterns/soulbound-tokens',
          'guides/developer/digital-assets/examples-patterns/nft-rental',
          'guides/developer/digital-assets/examples-patterns/kiosk',
          'guides/developer/digital-assets/examples-patterns/wasm-template',
        ],
      },
    ],
  },
  {
    type: 'category',
    label: 'Wallets',
    link: {
      type: 'doc',
      id: 'guides/developer/wallets/index',
    },
    items: [
      `guides/developer/wallets/what-is-a-wallet`,
      `guides/developer/wallets/slush`,
      `guides/developer/wallets/self-custody`,
      `guides/developer/wallets/zk-login-wallets`,
      'guides/developer/wallets/suilink'
    ],
  },
  {
    type: 'category',
    label: 'On-Chain Primitives',
    link: {
      type: 'doc',
      id: 'guides/developer/on-chain-primitives/index',
    },
    items: [
      'guides/developer/on-chain-primitives/access-time',
      'guides/developer/on-chain-primitives/randomness-onchain',
    ],
  },
  {
    type: 'category',
    label: 'Cryptography',
    link: {
      type: 'doc',
      id: 'guides/developer/cryptography/index',
    },
    items: [
      'guides/developer/cryptography/signing',
      'guides/developer/cryptography/groth16',
      'guides/developer/cryptography/hashing',
      'guides/developer/cryptography/ecvrf',
      'guides/developer/cryptography/multisig',
      {
        type: 'category',
        label: 'zkLogin Integration Guide',
        link: {
          type: 'doc',
          id: 'guides/developer/cryptography/zklogin-integration/index',
        },
        items: [
          'guides/developer/cryptography/zklogin-integration/zklogin',
          'guides/developer/cryptography/zklogin-integration/zklogin-integration',
          'guides/developer/cryptography/zklogin-integration/developer-account',
          'guides/developer/cryptography/zklogin-integration/zklogin-example',
        ],
      },
    ],
  },
  {
    type: 'category',
    label: 'Nautilus',
    link: {
      type: 'doc',
      id: 'guides/developer/nautilus/index',
    },
    items: [
      'guides/developer/nautilus/nautilus-overview',
      'guides/developer/nautilus/nautilus-design',
      'guides/developer/nautilus/using-nautilus',
      'guides/developer/nautilus/customize-nautilus',
      'guides/developer/nautilus/community-dev-tools',
      'guides/developer/nautilus/seal',
    ],
  },
  {
    type: 'category',
    label: 'App Examples',
    link: {
      type: 'doc',
      id: 'guides/developer/app-examples/index',
    },
    items: [
      'guides/developer/app-examples/e2e-counter',
      'guides/developer/app-examples/client-tssdk',
      'guides/developer/app-examples/trustless-swap',
	  'guides/developer/app-examples/trustless-swap-frontend',
      'guides/developer/app-examples/coin-flip',
      'guides/developer/app-examples/reviews-rating',
      'guides/developer/app-examples/blackjack',
      'guides/developer/app-examples/plinko',
      'guides/developer/app-examples/tic-tac-toe',
      {
        type: 'category',
        label: 'Oracles',
        link: {
          type: 'doc',
          id: 'guides/developer/app-examples/oracle',
        },
        items: [
          'guides/developer/app-examples/weather-oracle',
        ],
      },
    ],
  },
  {
    type: 'category',
    label: 'Operator Guides',
    link: {
      type: 'doc',
      id: 'guides/operator/index',
    },
    items: [
      'guides/operator/sui-full-node',
      'guides/operator/genesis',
      'guides/operator/monitoring',
      'guides/operator/alerts',
      'guides/operator/updates',
      'guides/operator/data-management',
      'guides/operator/snapshots',
      'guides/operator/archives',
      'guides/operator/exchange-integration',
      'guides/operator/bridge-node-configuration',
      'guides/operator/indexer-stack-setup',
      'guides/operator/archival-stack-setup',
      'guides/operator/remote-store-setup',
      {
        type: 'category',
        label: 'Sui Validator Nodes',
        link: {
          type: 'doc',
          id: 'guides/operator/validator-index',
        },
        items: [
          {
            type: 'autogenerated',
            dirName: 'guides/operator/validator',
          },
        ],
      },
    ],
  },
  {
    type: 'category',
    label: 'SuiPlay0X1',
    collapsed: true,
    link: {
      type: 'doc',
      id: 'guides/suiplay0x1/index',
    },
    items: [
      'guides/suiplay0x1/integration',
      'guides/suiplay0x1/migration-strategies',
      'guides/suiplay0x1/wallet-integration',
      'guides/suiplay0x1/best-practices',
    ],
  },
  'tooling',
  'guides/developer/dev-cheat-sheet',
  'guides/developer/move-best-practices',
  'guides/developer/common-errors',
];

export default guides;
