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
				],
			},
			'guides/developer/getting-started/configure-sui-client',
			'guides/developer/getting-started/get-address',
			'guides/developer/getting-started/get-coins',
			'guides/developer/getting-started/hello-world',
			'guides/developer/getting-started/next-steps',
		],
	},
	{
		type: 'category',
		label: 'Sui Essentials',
		link: {
			type: 'doc',
			id: 'guides/developer/sui-101',
		},
		items: [
			'guides/developer/sui-101/object-ownership',
			'guides/developer/sui-101/using-events',
			'guides/developer/sui-101/local-network',
			'guides/developer/sui-101/connect',
			'guides/developer/sui-101/data-serving',
			'guides/developer/sui-101/access-time',
			'guides/developer/sui-101/sign-and-send-txn',
			'guides/developer/sui-101/sponsor-txn',
			'guides/developer/sui-101/avoid-equivocation',
			'guides/developer/sui-101/common-errors',

			{
				type: 'category',
				label: 'Working with PTBs',
				link: {
					type: 'doc',
					id: 'guides/developer/sui-101/working-with-ptbs',
				},
				items: [
					'guides/developer/sui-101/building-ptb',
					'guides/developer/sui-101/coin-mgt',
					'guides/developer/sui-101/simulating-refs',
				],
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
		label: 'Advanced',
		link: {
			type: 'doc',
			id: 'guides/developer/advanced',
		},
		items: [
			'guides/developer/advanced/move-2024-migration',
			{
				type: 'category',
				label: 'Custom Indexer',
				link: {
					type: 'doc',
					id: 'guides/developer/advanced/custom-indexer',
				},
				items: [
					'guides/developer/advanced/custom-indexer/build',
					'guides/developer/advanced/custom-indexer/indexer-walrus',
					'guides/developer/advanced/custom-indexer/indexer-data-integration',
					'guides/developer/advanced/custom-indexer/indexer-runtime-perf',
				],
			},
			'guides/developer/advanced/randomness-onchain',
			'guides/developer/advanced/graphql-rpc',
			'guides/developer/advanced/local-fee-markets',
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
	'guides/developer/starter-templates',
	'guides/developer/zklogin-onboarding',
	'guides/developer/dev-cheat-sheet',
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