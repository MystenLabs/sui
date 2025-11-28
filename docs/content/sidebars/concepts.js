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
					'concepts/object-ownership/party',
					'concepts/object-ownership/shared',
					'concepts/object-ownership/wrapped',
				],
			},
			{
				type: 'category',
				label: 'Transfers',
				link: {
					type: 'doc',
					id: 'concepts/transfers',
				},
				items: [
					'concepts/transfers/custom-rules',
					'concepts/transfers/transfer-policies',
					'concepts/transfers/transfer-to-object',
				],
			},
			'concepts/versioning',
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
			{
				type: 'category',
				label: 'Dynamic Fields',
				link: {
					type: 'doc',
					id: 'concepts/dynamic-fields',
				},
				items: ['concepts/dynamic-fields/tables-bags'],
			},
			'concepts/sui-move-concepts/derived-objects',
			'concepts/sui-move-concepts/conventions',
		],
	},
	{
		type: 'category',
		label: 'Data Access',
		link: {
			type: 'doc',
			id: 'concepts/data-access/data-serving',
		},
		items: [
			'concepts/data-access/grpc-overview',
			{
				type: 'category',
				label: 'GraphQL and Indexer Framework',
				link: {
					type: 'doc',
					id: 'concepts/data-access/graphql-indexer',
				},
				items: [
					'concepts/data-access/graphql-rpc',
					'concepts/data-access/custom-indexing-framework',
					'concepts/data-access/pipeline-architecture',
				],
			},
			'concepts/data-access/archival-store',
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
			{
				type: 'category',
				label: 'Nautilus',
				link: {
					type: 'doc',
					id: 'concepts/cryptography/nautilus',
				},
				items: [
					'concepts/cryptography/nautilus/nautilus-design',
					'concepts/cryptography/nautilus/using-nautilus',
				],
			},
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
	'concepts/gaming',
	'concepts/research-papers',
];
export default concepts;
