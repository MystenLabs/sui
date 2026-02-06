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
			{
				type: 'category',
				label: 'Contract Information',
				link: {
					type: 'doc',
					id: 'standards/deepbookv3/contract-information',
				},
				items: [
					'standards/deepbookv3/contract-information/balance-manager',
					'standards/deepbookv3/contract-information/permissionless-pool',
					'standards/deepbookv3/contract-information/query-the-pool',
					'standards/deepbookv3/contract-information/orders',
					'standards/deepbookv3/contract-information/swaps',
					'standards/deepbookv3/contract-information/flash-loans',
					'standards/deepbookv3/contract-information/staking-governance',
					'standards/deepbookv3/contract-information/referral',
					'standards/deepbookv3/contract-information/ewma',
				],
			},
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
			'standards/deepbook-margin/margin-risks',
			{
				type: 'category',
				label: 'Contract Information',
				link: {
					type: 'doc',
					id: 'standards/deepbook-margin/contract-information',
				},
				items: [
					'standards/deepbook-margin/contract-information/risk-ratio',
					'standards/deepbook-margin/contract-information/margin-manager',
					'standards/deepbook-margin/contract-information/margin-pool',
					'standards/deepbook-margin/contract-information/interest-rates',
					'standards/deepbook-margin/contract-information/orders',
					'standards/deepbook-margin/contract-information/tpsl',
					'standards/deepbook-margin/contract-information/supply-referral',
					'standards/deepbook-margin/contract-information/maintainer',
				],
			},
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
