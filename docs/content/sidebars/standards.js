// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const standards = [
	'standards',
	'standards/coin',
	'standards/currency',
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
		label: 'DeepBookV3',
		link: {
			type: 'doc',
			id: 'standards/deepbook',
		},
		items: [
			'standards/deepbookv3/design',
			'standards/deepbookv3/contract-information',
			'standards/deepbookv3/balance-manager',
			'standards/deepbookv3/permissionless-pool',
			'standards/deepbookv3/query-the-pool',
			'standards/deepbookv3/orders',
			'standards/deepbookv3/swaps',
			'standards/deepbookv3/flash-loans',
			'standards/deepbookv3/staking-governance',
			'standards/deepbookv3/referral',
			'standards/deepbookv3/ewma',
			'standards/deepbookv3-indexer',
			{
				type: 'category',
				label: 'SDK',
				link: {
					type: 'doc',
					id: 'standards/deepbookv3-sdk',
				},
				items: [
					'standards/deepbookv3-sdk/balance-manager',
					'standards/deepbookv3-sdk/pools',
					'standards/deepbookv3-sdk/orders',
					'standards/deepbookv3-sdk/swaps',
					'standards/deepbookv3-sdk/flash-loans',
					'standards/deepbookv3-sdk/staking-governance',
				],
			},
		],
	},
	{
		type: 'category',
		label: 'DeepBook Margin',
		link: {
			type: 'doc',
			id: 'standards/deepbook-margin',
		},
		items: [
			'standards/deepbook-margin/design',
			'standards/deepbook-margin/contract-information',
			'standards/deepbook-margin/margin-manager',
			'standards/deepbook-margin/margin-pool',
			'standards/deepbook-margin/orders',
			'standards/deepbook-margin/tpsl',
			'standards/deepbook-margin/maintainer',
			'standards/deepbook-margin-indexer',
			{
				type: 'category',
				label: 'SDK',
				link: {
					type: 'doc',
					id: 'standards/deepbook-margin-sdk',
				},
				items: [
					'standards/deepbook-margin-sdk/margin-manager',
					'standards/deepbook-margin-sdk/margin-pool',
					'standards/deepbook-margin-sdk/orders',
					'standards/deepbook-margin-sdk/tpsl',
					'standards/deepbook-margin-sdk/maintainer',
				],
			},
		],
	},
	'standards/display',
	'standards/payment-kit',
	'standards/sagat',
	'standards/wallet-standard',
];
export default standards;
