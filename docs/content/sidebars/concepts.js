// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const concepts = [
	'concepts',
	'concepts/sui-for-ethereum',
	'concepts/sui-for-solana',
	{
		type: 'category',
		label: 'Architecture',
		link: {
			type: 'doc',
			id: 'concepts/architecture',
		},
		items: [
			'concepts/sui-architecture/networks',
			'concepts/sui-architecture/sui-storage',
			'concepts/sui-architecture/consensus',
			'concepts/sui-architecture/epochs',
			'concepts/sui-architecture/sui-security',
			'concepts/sui-architecture/protocol-upgrades',
		],
	},
	{
		type: 'category',
		label: 'Transactions',
		items: [
			'concepts/transactions/transaction-lifecycle',
			'concepts/transactions/inputs-and-results',
			'concepts/transactions/gas-smashing',
			'concepts/transactions/transaction-auth',
		],
	},
	{
		type: 'category',
		label: 'Tokenomics',
		link: {
			type: 'doc',
			id: 'concepts/tokenomics',
		},
		items: [
			'concepts/tokenomics/staking-unstaking',
			'concepts/tokenomics/sui-bridging',
			'concepts/tokenomics/gas-in-sui',
		],
	},
	'concepts/coin-mgt',

	{
		type: 'category',
		label: 'Move',
		link: {
			type: 'doc',
			id: 'concepts/sui-move-concepts',
		},
		items: [
			'concepts/sui-move-concepts/packages',
			'concepts/sui-move-concepts/conventions',
			'concepts/sui-move-concepts/move-2024-migration',
		],
	},
	{
		type: 'category',
		label: 'Accessing Data',
		link: {
			type: 'doc',
			id: 'concepts/data-access/data-serving',
		},
		items: [
			'concepts/data-access/grpc',
			'concepts/data-access/graphql-indexer',
			'concepts/data-access/graphql-rpc',
		],
	},
	{
		type: 'category',
		label: 'Cryptography',
		link: {
			type: 'doc',
			id: 'concepts/cryptography',
		},
		items: [
			'concepts/cryptography/zklogin',
			'concepts/cryptography/passkeys',
			'concepts/cryptography/nautilus/nautilus-design',
			'concepts/cryptography/system/checkpoint-verification',
			/*{
				type: 'category',
				label: 'System',
				link: {
					type: 'doc',
					id: 'concepts/cryptography/system',
				},
				items: [
					'concepts/cryptography/system/validator-signatures',
					'concepts/cryptography/system/intents-for-validation',
				],
			},*/
		],
	},
	'concepts/gaming',
	'concepts/research-papers',
];
export default concepts;
