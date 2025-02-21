// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const concepts = [
	'concepts',
	'concepts/components',
	{
		type: 'category',
		label: 'App Developers',
		link: {
			type: 'doc',
			id: 'concepts/app-devs',
		},
		items: [
			{
				type: 'category',
				label: 'Object Model',
				link: {
					type: 'doc',
					id: 'concepts/object-model',
				},
				items: [
					{
						type: 'category',
						label: 'Object Ownership',
						link: {
							type: 'doc',
							id: 'concepts/object-ownership',
						},
						items: [
							'concepts/object-ownership/address-owned',
							'concepts/object-ownership/immutable',
							'concepts/object-ownership/shared',
							'concepts/object-ownership/wrapped',
						],
					},
					{
						type: 'category',
						label: 'Dynamic Fields',
						link: {
							type: 'doc',
							id: 'concepts/dynamic-fields',
						},
						items: ['concepts/dynamic-fields/tables-bags'],
					},
					{
						type: 'category',
						label: 'Transfers',
						link: {
							type: 'doc',
							id: 'concepts/transfers',
						},
						items: ['concepts/transfers/custom-rules', 'concepts/transfers/transfer-to-object'],
					},
					'concepts/versioning',
				],
			},
			{
				type: 'category',
				label: 'Move Overview',
				link: {
					type: 'doc',
					id: 'concepts/sui-move-concepts',
				},
				items: [
					{
						type: 'category',
						label: 'Packages',
						link: {
							type: 'doc',
							id: 'concepts/sui-move-concepts/packages',
						},
						items: [
							'concepts/sui-move-concepts/packages/upgrade',
							'concepts/sui-move-concepts/packages/custom-policies',
							'concepts/sui-move-concepts/packages/automated-address-management',
						],
					},
					'concepts/sui-move-concepts/conventions',
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
					'concepts/transactions/prog-txn-blocks',
					'concepts/transactions/sponsored-transactions',
					'concepts/transactions/gas-smashing',
				],
			},
			'concepts/graphql-rpc',
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
			{
				type: 'category',
				label: 'Transaction Authentication',
				link: {
					type: 'doc',
					id: 'concepts/cryptography/transaction-auth',
				},
				items: [
					'concepts/cryptography/transaction-auth/keys-addresses',
					'concepts/cryptography/transaction-auth/signatures',
					'concepts/cryptography/transaction-auth/multisig',
					'concepts/cryptography/transaction-auth/offline-signing',
					'concepts/cryptography/transaction-auth/intent-signing',
				],
			},
			'concepts/cryptography/zklogin',
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
					'concepts/cryptography/system/checkpoint-verification',
				],
			},*/
		],
	},
	{
		type: 'category',
		label: 'Sui Architecture',
		link: {
			type: 'doc',
			id: 'concepts/sui-architecture',
		},
		items: [
			'concepts/sui-architecture/high-level',
			'concepts/sui-architecture/sui-storage',
			'concepts/sui-architecture/sui-security',
			'concepts/sui-architecture/transaction-lifecycle',
			'concepts/sui-architecture/consensus',
			'concepts/sui-architecture/indexer-functions',
			'concepts/sui-architecture/epochs',
			'concepts/sui-architecture/protocol-upgrades',
			'concepts/sui-architecture/data-management-things',
			'concepts/sui-architecture/staking-rewards',
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
			'concepts/tokenomics/gas-pricing',
			'concepts/tokenomics/gas-in-sui',
			'concepts/tokenomics/vesting-strategies'
		],
	},
	'concepts/sui-bridge',
	'concepts/research-papers',
];
module.exports = concepts;
