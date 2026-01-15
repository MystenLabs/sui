// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const concepts = [
	'concepts',
	'concepts/sui-for-ethereum',
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
		link: {
			type: 'doc',
			id: 'concepts/transactions',
		},
		items: [
			'concepts/transactions/transaction-lifecycle',
			'concepts/transactions/prog-txn-blocks',
			'concepts/transactions/sponsored-transactions',
			'concepts/transactions/gas-smashing',
			'concepts/transactions/coin-mgt',
			{
				type: 'category',
				label: 'Transaction Authentication',
				link: {
					type: 'doc',
					id: 'concepts/transactions/transaction-auth',
				},
				items: [
					'concepts/transactions/transaction-auth/multisig',
					'concepts/transactions/transaction-auth/offline-signing',
					'concepts/transactions/transaction-auth/intent-signing',
				],
			},
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
	'concepts/data-access/data-serving',
	'concepts/data-access/graphql-indexer',
	'concepts/data-access/graphql-rpc',
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
