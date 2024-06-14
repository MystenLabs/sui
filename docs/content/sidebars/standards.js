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
			'standards/deepbook/design',
			'standards/deepbook/orders',
			'standards/deepbook/pools',
			'standards/deepbook/query-the-pool',
			'standards/deepbook/routing-a-swap',
			'standards/deepbook/trade-and-swap',
		],
	},
	'standards/display',
	'standards/wallet-standard',
];
module.exports = standards;
