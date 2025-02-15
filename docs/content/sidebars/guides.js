// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const guides = [
	{
		type: 'doc',
		label: 'Guides',
		id: 'guides',
	},
	{
		type: 'category',
		label: 'Developer Guides',
		collapsed: false,
		link: {
			type: 'doc',
			id: 'guides/developer',
		},
		items: [
			{
				type: 'category',
				label: 'Getting Started',
				collapsed: false,
				link: {
					type: 'doc',
					id: 'guides/developer/getting-started',
				},
				items: [
					'guides/developer/getting-started/sui-install',
					'guides/developer/getting-started/connect',
					'guides/developer/getting-started/local-network',
					'guides/developer/getting-started/get-address',
					'guides/developer/getting-started/get-coins',
					'guides/developer/getting-started/data-serving',
				],
			},
			{
				type: 'category',
				label: 'Your First Sui dApp',
				link: {
					type: 'doc',
					id: 'guides/developer/first-app',
				},
				items: [
					'guides/developer/first-app/write-package',
					'guides/developer/first-app/build-test',
					'guides/developer/first-app/publish',
					'guides/developer/first-app/debug',
					'guides/developer/first-app/client-tssdk',
				],
			},
			{
				type: 'category',
				label: 'Sui 101',
				link: {
					type: 'doc',
					id: 'guides/developer/sui-101',
				},
				items: [
					'guides/developer/sui-101/shared-owned',
					'guides/developer/sui-101/using-events',
					'guides/developer/sui-101/access-time',
					'guides/developer/sui-101/sign-and-send-txn',
					'guides/developer/sui-101/sponsor-txn',
					'guides/developer/sui-101/avoid-equivocation',
					{
						type: 'category',
						label: 'Working with PTBs',
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
				label: 'Coins and Tokens',
				link: {
					type: 'doc',
					id: 'guides/developer/coin',
				},
				items: [
					'guides/developer/coin/regulated',
					'guides/developer/coin/in-game-token',
					'guides/developer/coin/loyalty',
				],
			},
			'guides/developer/stablecoins',
			{
				type: 'category',
				label: 'NFTs',
				link: {
					type: 'doc',
					id: 'guides/developer/nft',
				},
				items: ['guides/developer/nft/nft-rental', 'guides/developer/nft/asset-tokenization'],
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
				label: 'Advanced Topics',
				link: {
					type: 'doc',
					id: 'guides/developer/advanced',
				},
				items: [
					/*{
						type: 'category',
						label: 'Efficient Smart Contracts',
						link: {
							type: 'doc',
							id: 'guides/developer/advanced/efficient-smart-contracts',
						},
						items: ['guides/developer/advanced/min-gas-fees'],
					},*/
					'guides/developer/advanced/move-2024-migration',
					'guides/developer/advanced/custom-indexer',
					'guides/developer/advanced/randomness-onchain',
					'guides/developer/getting-started/graphql-rpc',
					'guides/developer/advanced/graphql-migration',
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
					// 'guides/developer/app-examples/recaptcha',
					// 'guides/developer/app-examples/trusted-swap',
					// 'guides/developer/app-examples/turnip-town',
					// 'guides/developer/app-examples/auction',
				],
			},
			'guides/developer/starter-templates',
			'guides/developer/zklogin-onboarding',
			'guides/developer/dev-cheat-sheet',
		],
	},
	{
		type: 'category',
		label: 'Operator Guides',
		link: {
			type: 'doc',
			id: 'guides/operator',
		},
		items: [
			'guides/operator/sui-full-node',
			'guides/operator/validator-config',
			'guides/operator/genesis',
			'guides/operator/monitoring',
			'guides/operator/updates',
			'guides/operator/data-management',
			'guides/operator/snapshots',
			'guides/operator/archives',
			'guides/operator/node-tools',
			'guides/operator/exchange-integration',
			'guides/operator/bridge-node-configuration',
			'guides/operator/validator-committee',
			'guides/operator/validator-tasks',
		],
	},
];
module.exports = guides;
