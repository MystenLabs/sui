// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import references from './references.js';

export default {
  referencesSidebar: references,
  developSidebar: [
    'develop',
    {
      type: 'category',
      label: 'Sui Architecture',
      link: { type: 'doc', id: 'develop/sui-architecture/index' },
      items: [
        'develop/sui-architecture/components',
        'develop/sui-architecture/networks',
        'develop/sui-architecture/consensus',
        'develop/sui-architecture/tokenomics-overview',
        'develop/sui-architecture/object-model',
        'develop/sui-architecture/epochs',
        'develop/sui-architecture/checkpoint-verification',
        'develop/sui-architecture/sui-storage',
        'develop/sui-architecture/sui-security',
        'develop/sui-architecture/protocol-upgrades',
      ],
    },
    {
      type: 'category',
      label: 'Using Objects',
      link: { type: 'doc', id: 'develop/objects/index' },
      items: [
         {
          type: 'category',
          label: 'Types of Object Ownership',
          link: { type: 'doc', id: 'develop/objects/object-ownership/index' },
          items: [
            'develop/objects/object-ownership/address-owned',
            'develop/objects/object-ownership/shared',
            'develop/objects/object-ownership/immutable',
            'develop/objects/object-ownership/wrapped',
            'develop/objects/object-ownership/party',
          ],
        },
        'develop/objects/derived-objects',
        'develop/objects/dynamic-fields',
        'develop/objects/versioning',
        {
          type: 'category',
          label: 'Object Display',
          link: { type: 'doc', id: 'develop/objects/display/index' },
          items: [
            'develop/objects/display/display-overview',
            'develop/objects/display/using-display',
            'develop/objects/display/display-preview',
            ],
          },
          {
            type: 'category',
            label: 'Transferring Objects',
            link: { type: 'doc', id: 'develop/objects/transfers/index' },
            items: [
              'develop/objects/transfers/custom-rules',
              'develop/objects/transfers/transfer-policies',
              'develop/objects/transfers/transfer-to-object',
              'develop/objects/transfers/simulating-refs',
            ],
          },
        ],
      },
      {
        type: 'category',
        label: 'Writing Move Packages',
        link: { type: 'doc', id: 'develop/write-move/index' },
        items: [
            'develop/write-move/package-overview',
            'develop/write-move/sui-move-concepts',
            'develop/write-move/move-fundamentals',
            'develop/write-move/move-best-practices',
            {
            type: 'link',
            label: 'Move Book',
            href: 'https://move-book.com/',
          },
        ]
      },
      {
        type: 'category',
        label: 'Deploying and Upgrading Packages',
        link: { type: 'doc', id: 'develop/publish-upgrade-packages/index' },
        items: [
            'develop/publish-upgrade-packages/deploy',
            'develop/publish-upgrade-packages/upgrade',
            'develop/publish-upgrade-packages/custom-policies',
            'develop/publish-upgrade-packages/versioning',
        ]
      },
      {
        type: 'category',
        label: 'Managing Packages',
        link: { type: 'doc', id: 'develop/manage-packages/index' },
        items: [
            'develop/manage-packages/move-package-management',
            'develop/manage-packages/automated-address-management',
        ]
      },      
      {
        type: 'category',
        label: 'Testing and Debugging',
        link: { type: 'doc', id: 'develop/testing-debugging/index' },
        items: [
            'develop/testing-debugging/testing',
            'develop/testing-debugging/common-errors',
        ]
      },
    {
      type: 'category',
      label: 'Building Transactions',
      link: { type: 'doc', id: 'develop/transactions/index' },
      items: [
        'develop/transactions/txn-overview',
        'develop/transactions/transaction-lifecycle',
        {
          type: 'category',
          label: 'Programmable Transaction Blocks',
          link: { type: 'doc', id: 'develop/transactions/ptbs/index' },
          items: [
            'develop/transactions/ptbs/prog-txn-blocks',
            'develop/transactions/ptbs/building-ptb',
            'develop/transactions/ptbs/inputs-and-results',
          ],
        },
        {
          type: 'category',
          label: 'Transaction Authentication',
          link: { type: 'doc', id: 'develop/transactions/transaction-auth/index' },
          items: [
            'develop/transactions/transaction-auth/auth-overview',
            'develop/transactions/transaction-auth/multisig',
            'develop/transactions/transaction-auth/intent-signing',
            'develop/transactions/transaction-auth/offline-signing',
            'develop/transactions/transaction-auth/address-aliases',
          ],
        },
      ],
    },
    {
      type: 'category',
      label: 'Paying for Transactions',
      link: { type: 'doc', id: 'develop/transaction-payment/index' },
      items: [
        'develop/transaction-payment/gas-in-sui',
        'develop/transaction-payment/local-fee-markets',
        'develop/transaction-payment/gasless-transactions',
        'develop/transaction-payment/sponsor-txn',
        'develop/transaction-payment/gas-smashing',
      ],
    },
    {
      type: 'category',
      label: 'Accessing Data',
      link: { type: 'doc', id: 'develop/accessing-data/index' },
      items: [
        'develop/accessing-data/data-serving',
        {
          type: 'category',
          label: 'gRPC',
          link: { type: 'doc', id: 'develop/accessing-data/grpc/index' },
          items: [
            'develop/accessing-data/grpc/what-is-grpc',
            'develop/accessing-data/grpc/using-grpc',
          ],
        },
        {
          type: 'category',
          label: 'GraphQL',
          link: { type: 'doc', id: 'develop/accessing-data/graphql/index' },
          items: [
            'develop/accessing-data/graphql/graphql-rpc',
            'develop/accessing-data/graphql/query-with-graphql',
          ],
        },
        {
          type: 'category',
          label: 'Archival Service',
          link: { type: 'doc', id: 'develop/accessing-data/archival-store/index' },
          items: [
            'develop/accessing-data/archival-store/what-is-archival-store',
            'develop/accessing-data/archival-store/using-archival-store',
          ],
        },
        'develop/accessing-data/using-events',
        'develop/accessing-data/authenticated-events',
        {
          type: 'category',
          label: 'Custom Indexing Framework',
          link: { type: 'doc', id: 'develop/accessing-data/custom-indexer/index' },
          items: [
            'develop/accessing-data/custom-indexer/custom-indexers',
            'develop/accessing-data/custom-indexer/pipeline-architecture',
            'develop/accessing-data/custom-indexer/build',
            'develop/accessing-data/custom-indexer/bring-your-own-store',
            'develop/accessing-data/custom-indexer/indexer-data-integration',
            'develop/accessing-data/custom-indexer/indexer-runtime-perf',
          ],
        },
      ],
    },
    {
      type: 'category',
      label: 'Cryptography',
      link: { type: 'doc', id: 'develop/cryptography/index' },
      items: [
        'develop/cryptography/signing',
        'develop/cryptography/hashing',
        'develop/cryptography/groth16',
        'develop/cryptography/ecvrf',
        'develop/cryptography/passkeys',
      ],
    },
    {
      type: 'category',
      label: 'Security',
      link: { type: 'doc', id: 'develop/security/index' },
      items: [
        'develop/security/best-practices',
      ],
    },
  ],

  gettingStartedSidebar: [
    'getting-started',
    'getting-started/agent-skills',
    {
      type: 'category',
      label: 'Hello, World!',
      collapsed: false,
      link: { type: 'doc', id: 'getting-started/onboarding/index' },
      items: [
          {
          type: 'category',
          label: 'Install Sui',
          link: { type: 'doc', id: 'getting-started/onboarding/sui-install'},
          items: [
              'getting-started/onboarding/install-binaries',
              'getting-started/onboarding/install-source',
              'getting-started/onboarding/local-network',
          ]
        },
        'getting-started/onboarding/configure-sui-client',
        'getting-started/onboarding/get-address',
        'getting-started/onboarding/get-coins',
        'getting-started/onboarding/hello-world',
        'getting-started/onboarding/app-frontends',
        'getting-started/onboarding/next-steps',
      ],
    },
    'getting-started/tooling',
    'getting-started/dev-cheat-sheet',
    'getting-started/sui-for-ethereum',
    'getting-started/sui-for-solana',
  ],

  onchainFinanceSidebar: [
      'onchain-finance',
      'onchain-finance/types-of-assets',
      {
        type: 'category',
        label: 'Asset Custody',
        link: { type: 'doc', id: 'onchain-finance/asset-custody/index' },
        items: [
          {
            type: 'category',
            label: 'Address Balances',
            link: { type: 'doc', id: 'onchain-finance/asset-custody/address-balances/index' },
            items: [
              'onchain-finance/asset-custody/address-balances/using-address-balances',
              'onchain-finance/asset-custody/address-balances/migrate-address-balances',
            ],
          },
          'onchain-finance/asset-custody/address-balance-migration',
          {
            type: 'category',
            label: 'Wallets',
            link: { type: 'doc', id: 'onchain-finance/asset-custody/wallets/index' },
            items: [
              'onchain-finance/asset-custody/wallets/wallet-standard',
              'onchain-finance/asset-custody/wallets/what-is-a-wallet',
              'onchain-finance/asset-custody/wallets/slush',
              'onchain-finance/asset-custody/wallets/self-custody',
              'onchain-finance/asset-custody/wallets/zk-login-wallets',
              'onchain-finance/asset-custody/wallets/suilink',
            ],
          },
        ],
      },
      {
        type: 'category',
        label: 'Fungible Tokens',
        link: { type: 'doc', id: 'onchain-finance/fungible-tokens/index' },
        items: [
          {
            type: 'category',
            label: 'Coin Standard',
            link: { type: 'doc', id: 'onchain-finance/fungible-tokens/coin' },
            items: [
                'onchain-finance/fungible-tokens/create-a-fungible-token-coin',
            ],
          },
          {
            type: 'category',
            label: 'Currency Standard',
            link: { type: 'doc', id: 'onchain-finance/fungible-tokens/currency' },
            items: [
                'onchain-finance/fungible-tokens/create-a-fungible-token',
            ],
          },
          'onchain-finance/fungible-tokens/integrating-with-stablecoins',
          'onchain-finance/fungible-tokens/regulated-tokens',
          'onchain-finance/fungible-tokens/token-vesting-strategies',
          'onchain-finance/fungible-tokens/sui-bridging',
        ],
      },
      {
        type: 'category',
        label: 'Tokenized Assets',
        link: { type: 'doc', id: 'onchain-finance/tokenized-assets/index' },
        items: [
          'onchain-finance/tokenized-assets/asset-tokenization',
          'onchain-finance/tokenized-assets/deploy-tokenized-asset',
          'onchain-finance/tokenized-assets/create-nft',
        ],
      },
      {
        type: 'category',
        label: 'Example Asset Patterns',
        link: { type: 'doc', id: 'onchain-finance/examples-patterns/index' },
        items: [
          'onchain-finance/examples-patterns/fixed-supply',
          'onchain-finance/examples-patterns/loyalty-tokens',
          'onchain-finance/examples-patterns/in-game-currency',
          'onchain-finance/examples-patterns/soulbound-tokens',
          'onchain-finance/examples-patterns/nft-rental',
          'onchain-finance/examples-patterns/kiosk',
          'onchain-finance/examples-patterns/wasm-template',
        ],
      },
      {
        type: 'category',
        label: 'Closed-Loop Token',
        link: { type: 'doc', id: 'onchain-finance/closed-loop-token/index' },
        items: [
          'onchain-finance/closed-loop-token/token-policy',
          'onchain-finance/closed-loop-token/action-request',
          'onchain-finance/closed-loop-token/rules',
          'onchain-finance/closed-loop-token/spending',
        ],
      },
      {
        type: 'category',
        label: 'Permissioned Asset Standard',
        link: { type: 'doc', id: 'onchain-finance/pas/index' },
        items: [
          'onchain-finance/pas/pas-architecture',
          'onchain-finance/pas/pas-workflows',
          'onchain-finance/pas/integrating-pas',
          'onchain-finance/pas/querying-assets',
        ],
      },
      {
      type: 'category',
      label: 'DeepBookV3',
      link: { type: 'doc', id: 'onchain-finance/deepbookv3/deepbook' },
      items: [
        'onchain-finance/deepbookv3/design',
        {
          type: 'category',
          label: 'Contract Information',
          link: { type: 'doc', id: 'onchain-finance/deepbookv3/contract-information' },
          items: [
            'onchain-finance/deepbookv3/contract-information/balance-manager',
            'onchain-finance/deepbookv3/contract-information/orders',
            'onchain-finance/deepbookv3/contract-information/flash-loans',
            'onchain-finance/deepbookv3/contract-information/swaps',
            'onchain-finance/deepbookv3/contract-information/staking-governance',
            'onchain-finance/deepbookv3/contract-information/permissionless-pool',
            'onchain-finance/deepbookv3/contract-information/query-the-pool',
            'onchain-finance/deepbookv3/contract-information/referral',
            'onchain-finance/deepbookv3/contract-information/ewma',
          ],
        },
        {
          type: 'category',
          label: 'DeepBookV3 SDK',
          link: { type: 'doc', id: 'onchain-finance/deepbookv3-sdk/deepbookv3-sdk' },
          items: [
            'onchain-finance/deepbookv3-sdk/balance-manager',
            'onchain-finance/deepbookv3-sdk/pools',
            'onchain-finance/deepbookv3-sdk/orders',
            'onchain-finance/deepbookv3-sdk/flash-loans',
            'onchain-finance/deepbookv3-sdk/swaps',
            'onchain-finance/deepbookv3-sdk/staking-governance',
          ],
        },
        'onchain-finance/deepbookv3/deepbookv3-indexer',
      ],
    },
    {
      type: 'category',
      label: 'DeepBook Margin',
      link: { type: 'doc', id: 'onchain-finance/deepbook-margin/deepbook-margin' },
      items: [
        'onchain-finance/deepbook-margin/design',
        'onchain-finance/deepbook-margin/margin-risks',
        {
          type: 'category',
          label: 'Contract Information',
          link: { type: 'doc', id: 'onchain-finance/deepbook-margin/contract-information' },
          items: [
            'onchain-finance/deepbook-margin/contract-information/margin-manager',
            'onchain-finance/deepbook-margin/contract-information/margin-pool',
            'onchain-finance/deepbook-margin/contract-information/orders',
            'onchain-finance/deepbook-margin/contract-information/maintainer',
            'onchain-finance/deepbook-margin/contract-information/tpsl',
            'onchain-finance/deepbook-margin/contract-information/interest-rates',
            'onchain-finance/deepbook-margin/contract-information/risk-ratio',
            'onchain-finance/deepbook-margin/contract-information/supply-referral',
          ],
        },
        {
          type: 'category',
          label: 'DeepBook Margin SDK',
          link: { type: 'doc', id: 'onchain-finance/deepbook-margin-sdk/deepbook-margin-sdk' },
          items: [
            'onchain-finance/deepbook-margin-sdk/margin-manager',
            'onchain-finance/deepbook-margin-sdk/margin-pool',
            'onchain-finance/deepbook-margin-sdk/orders',
            'onchain-finance/deepbook-margin-sdk/maintainer',
            'onchain-finance/deepbook-margin-sdk/tpsl',
          ],
        },
        'onchain-finance/deepbook-margin/deepbook-margin-indexer',
      ],
    },
    {
      type: 'category',
      label: 'Kiosk',
      link: { type: 'doc', id: 'onchain-finance/kiosk/index' },
      items: [
        'onchain-finance/kiosk/kiosk-example',
        'onchain-finance/kiosk/kiosk-apps',
      ],
    },
    'onchain-finance/payment-kit',
  ],

  suiStackSidebar: [
    'sui-stack',
    'sui-stack/on-chain-primitives/access-time',
    'sui-stack/on-chain-primitives/randomness-onchain',
    'sui-stack/sagat',
    'sui-stack/indexer-walrus',
    {
      type: 'category',
      label: 'Nautilus',
      link: { type: 'doc', id: 'sui-stack/nautilus/index' },
      items: [
        'sui-stack/nautilus/nautilus-overview',
        'sui-stack/nautilus/nautilus-design',
        'sui-stack/nautilus/using-nautilus',
        'sui-stack/nautilus/customize-nautilus',
        'sui-stack/nautilus/seal',
        'sui-stack/nautilus/community-dev-tools',
      ],
    },
    {
      type: 'category',
      label: 'zkLogin',
      link: { type: 'doc', id: 'sui-stack/zklogin-integration/index' },
      items: [
        'sui-stack/zklogin-integration/zklogin',
        'sui-stack/zklogin-integration/developer-account',
        'sui-stack/zklogin-integration/zklogin-example',
      ],
    },
    {
      type: 'category',
      label: 'SuiPlay0X1',
      link: { type: 'doc', id: 'sui-stack/suiplay0x1/index' },
      items: [
        'sui-stack/suiplay0x1/integration',
        'sui-stack/suiplay0x1/wallet-integration',
        'sui-stack/suiplay0x1/best-practices',
        'sui-stack/suiplay0x1/migration-strategies',
      ],
    },
  ],
  operatorSidebar: [ 
    'operators',
    'operators/genesis',
    'operators/observability',
    'operators/snapshots',
     {
      type: 'category',
      label: 'Full Nodes',
      link: { type: 'doc', id: 'operators/full-node/index', },
      items: [
        'operators/full-node/sui-full-node',
        'operators/full-node/monitoring',
        'operators/full-node/updates',
      ],
    },
    {
      type: 'category',
      label: 'Data Indexing and Archives',
      link: { type: 'doc', id: 'operators/data-management/index', },
      items: [
        'operators/data-management/managing-data',
        'operators/data-management/indexer-stack-setup',
        'operators/data-management/remote-store-setup',
        'operators/data-management/archival-stack-setup',
        'operators/data-management/archives',
      ],
    },
    {
		type: 'category',
		label: 'Validators',
		link: { type: 'doc', id: 'operators/validator/index', },
		items: [
      'operators/validator/validator-config',
      'operators/validator/validator-tasks',
      'operators/validator/node-tools',
      'operators/validator/validator-rewards',
      'operators/validator/alerts',
		],
	},
    'operators/exchange-integration',
    'operators/bridge-node-configuration',
],
};
