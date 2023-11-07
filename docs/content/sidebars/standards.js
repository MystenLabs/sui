// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const standards = [
	'standards',
	'standards/kiosk',
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
	{
		type: 'link',
		label: 'Wallet Kit',
		href: 'https://sui-typescript-docs.vercel.app/wallet-kit',
	},
];
module.exports = standards;
