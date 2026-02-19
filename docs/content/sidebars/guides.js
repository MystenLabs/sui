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
      id: 'guides/developer/getting-started',
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
    items: [
      'guides/developer/objects/object-model',
      {
        type: 'category',
        label: 'Object Ownership',
        link: {
          type: 'doc',
          id: 'guides/developer/objects/object-ownership',
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
        label: 'Transfers',
        link: {
          type: 'doc',
          id: 'guides/developer/objects/transfers',
        },
        items: [
          'guides/developer/objects/transfers/custom-rules',
          'guides/developer/objects/transfers/transfer-policies',
          'guides/developer/objects/transfers/transfer-to-object',
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
  				items: [
          'guides/developer/packages/move-package-management',
					'guides/developer/packages/upgrade',
					'guides/developer/packages/custom-policies',
					'guides/developer/packages/automated-address-management',
				],
			},
      {
				type: 'category',
				label: 'Transactions',
				link: {
					type: 'doc',
					id: 'guides/developer/transactions/txn-overview',
				},
        items: [
           {
            type: 'category',
            label: 'Programmable Transaction Blocks',
            link: {
              type: 'doc',
              id: 'guides/developer/transactions/prog-txn-blocks',
            },
              items: [
              'guides/developer/transactions/building-ptb',
              ],
            },
            'guides/developer/transactions/sign-and-send-txn',
            'guides/developer/transactions/sponsor-txn',
            {
            type: 'category',
            label: 'Transaction Authentication',
              items: [
              'guides/developer/transactions/transaction-auth/intent-signing',
              'guides/developer/transactions/transaction-auth/multisig',
              'guides/developer/transactions/transaction-auth/offline-signing',
              ],
            },
        ],
  },
  {
    type: 'category',
    label: 'Accessing Data',
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
          id: 'guides/developer/accessing-data/custom-indexing-framework',
        },
          items: [
            'guides/developer/accessing-data/custom-indexer/build',
            'guides/developer/accessing-data/custom-indexer/indexer-walrus',
            'guides/developer/accessing-data/custom-indexer/bring-your-own-store',
          ]
        },
    ],
  },
  {
    type: 'category',
    label: 'Currencies and Tokens',
    link: {
      type: 'doc',
      id: 'guides/developer/coin-index',
    },
    items: [
      'guides/developer/currency',
      'guides/developer/coin/regulated',
      'guides/developer/coin/in-game-token',
      'guides/developer/coin/loyalty',
      'guides/developer/coin/vesting-strategies',

    ],
  },
  {
    type: 'category',
    label: 'NFTs',
    link: {
      type: 'doc',
      id: 'guides/developer/nft-index',
    },
    items: [
      'guides/developer/nft',
      'guides/developer/nft/nft-soulbound',
      'guides/developer/nft/nft-rental',
      'guides/developer/nft/asset-tokenization',
    ],
  },
  {
    type: 'category',
    label: 'On-Chain Primitives',
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
      id: 'guides/developer/cryptography',
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
          id: 'guides/developer/cryptography/zklogin-integration',
        },
        items: [
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
      'guides/developer/nautilus/using-nautilus',
      'guides/developer/nautilus/customize-nautilus',
      'guides/developer/nautilus/marlin',
      'guides/developer/nautilus/seal',
    ],
  },
  {
    type: 'category',
    label: 'Wallets',
    items: [
      'guides/developer/wallets/suilink',
    ],
  },
  {
    type: 'category',
    label: 'App Examples',
    link: {
      type: 'doc',
      id: 'guides/developer/app-examples',
    },
    items: [
      'guides/developer/app-examples/e2e-counter',
      'guides/developer/app-examples/client-tssdk',
      'guides/developer/app-examples/trustless-swap',
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
          'guides/developer/app-examples/meta-pricing-oracle',
        ],
      },
    ],
  },
  'guides/developer/dev-cheat-sheet',
  'guides/developer/common-errors',
  {
    type: 'category',
    label: 'Operator Guides',
    link: {
      type: 'doc',
      id: 'guides/operator',
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
      id: 'guides/suiplay0x1',
    },
    items: [
      'guides/suiplay0x1/integration',
      'guides/suiplay0x1/migration-strategies',
      'guides/suiplay0x1/wallet-integration',
      'guides/suiplay0x1/best-practices',
    ],
  },
];

export default guides;
