// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const standards = [
	'standards',
	'standards/coin',
	{
		type: 'category',
		label: 'Closed-Loop Token',
		link: {
			type: 'doc',
			id: 'standards/closed-loop-token',
		},
		items: [
			'standards/closed-loop-token/action-request',
			'standards/closed-loop-token/token-policy',
			'standards/closed-loop-token/spending',
			'standards/closed-loop-token/rules',
			'standards/closed-loop-token/coin-token-comparison',
		],
	},
	'standards/kiosk',
	'standards/kiosk-apps',
	{
		type: 'category',
		label: 'DeepBook',
		link: {
			type: 'doc',
			id: 'standards/deepbook',
		},
		items: [
			{
				type: 'category',
				label: 'DeepBookV3',
				link: {
					type: 'doc',
					id: 'standards/deepbookv3',
				},
				items: [
					'standards/deepbookv3/design',
					'standards/deepbookv3/balance-manager',
					'standards/deepbookv3/query-the-pool',
					'standards/deepbookv3/orders',
					'standards/deepbookv3/swaps',
					'standards/deepbookv3/flash-loans',
					'standards/deepbookv3/staking-governance',
				],
			},
			'standards/deepbookv3-indexer',
			{
				type: 'category',
				label: 'DeepBookV3 SDK',
				link: {
					type: 'doc',
					id: 'standards/deepbookv3-sdk',
				},
				items: [
					'standards/deepbookv3-sdk/flash-loans',
					'standards/deepbookv3-sdk/orders',
					'standards/deepbookv3-sdk/pools',
					'standards/deepbookv3-sdk/staking-governance',
					'standards/deepbookv3-sdk/swaps',
				],
			},
			{
				type: 'category',
				label: 'DeepBookV2',
				link: {
					type: 'doc',
					id: 'standards/deepbookv2',
				},
				items: [
					'standards/deepbookv2/design',
					'standards/deepbookv2/orders',
					'standards/deepbookv2/pools',
					'standards/deepbookv2/query-the-pool',
					'standards/deepbookv2/routing-a-swap',
					'standards/deepbookv2/trade-and-swap',
				],
			},
		],
	},
	'standards/display',
	'standards/wallet-standard',
];
module.exports = standards;
