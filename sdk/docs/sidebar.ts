// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { Sidebar } from 'vocs';

export const sidebar = {
	'/typescript': [
		{ text: 'Quick Start', link: '/typescript' },
		{ text: 'Install', link: '/typescript/install' },
		{ text: 'Hello, Sui', link: '/typescript/hello-sui' },
		{ text: 'Faucet', link: '/typescript/faucet' },
		{ text: 'SuiClient', link: '/typescript/sui-client' },
		{
			text: 'Transaction Building',
			// link: '/typescript/transaction-building',
			items: [
				{ text: 'Sui Programmable Transaction Blocks Basics', link: '' },
				{ text: 'Paying for Sui Transactions with Gas Coins', link: '' },
				{ text: 'Sponsored Transaction Blocks' },
				{ text: 'Building Offline' },
			],
		},
		{
			text: 'Cryptography',
			items: [{ text: 'Key pairs' }, { text: 'Multi-Signature Transactions' }],
		},
		{ text: 'Utils', link: '/typescript/utils' },
		{ text: 'BCS', link: '/typescript/bcs' },
		{
			text: 'Owned Object Pool',
			link: '/typescript/owned-object-pool',
			collapsed: true,
			items: [
				{ text: 'Sui Owned Object Pool Quick Start', link: '' },
				{ text: 'Introduction', link: '' },
				{ text: 'Local Development', link: '' },
				{ text: 'Custom SplitStrategy', link: '' },
				{ text: 'End-to-End Examples', link: '' },
			],
		},
	],
	'/op-stack': {
		backLink: true,
		items: [
			{
				text: 'OP Stack',
				items: [
					{
						text: 'Getting started',
						link: '/op-stack',
					},
					{ text: 'Client', link: '/op-stack/client' },
					{ text: 'Chains', link: '/op-stack/chains' },
				],
			},
			{
				text: 'Guides',
				items: [
					{
						text: 'Deposits',
						link: '/op-stack/guides/deposits',
					},
					{
						text: 'Withdrawals',
						link: '/op-stack/guides/withdrawals',
					},
				],
			},
			{
				text: 'L2 Public Actions',
				items: [
					{
						text: 'buildDepositTransaction',
						link: '/op-stack/actions/buildDepositTransaction',
					},
					{
						text: 'buildProveWithdrawal',
						link: '/op-stack/actions/buildProveWithdrawal',
					},
					{
						text: 'estimateContractL1Fee',
						link: '/op-stack/actions/estimateContractL1Fee',
					},
					{
						text: 'estimateContractL1Gas',
						link: '/op-stack/actions/estimateContractL1Gas',
					},
					{
						text: 'estimateContractTotalFee',
						link: '/op-stack/actions/estimateContractTotalFee',
					},
					{
						text: 'estimateContractTotalGas',
						link: '/op-stack/actions/estimateContractTotalGas',
					},
					{
						text: 'estimateInitiateWithdrawalGas',
						link: '/op-stack/actions/estimateInitiateWithdrawalGas',
					},
					{
						text: 'estimateL1Fee',
						link: '/op-stack/actions/estimateL1Fee',
					},
					{
						text: 'estimateL1Gas',
						link: '/op-stack/actions/estimateL1Gas',
					},
					{
						text: 'estimateTotalFee',
						link: '/op-stack/actions/estimateTotalFee',
					},
					{
						text: 'estimateTotalGas',
						link: '/op-stack/actions/estimateTotalGas',
					},
				],
			},
			{
				text: 'L2 Wallet Actions',
				items: [
					{
						text: 'initiateWithdrawal',
						link: '/op-stack/actions/initiateWithdrawal',
					},
				],
			},
			{
				text: 'L1 Public Actions',
				items: [
					{
						text: 'buildInitiateWithdrawal',
						link: '/op-stack/actions/buildInitiateWithdrawal',
					},
					{
						text: 'estimateDepositTransactionGas',
						link: '/op-stack/actions/estimateDepositTransactionGas',
					},
					{
						text: 'estimateFinalizeWithdrawalGas',
						link: '/op-stack/actions/estimateFinalizeWithdrawalGas',
					},
					{
						text: 'estimateProveWithdrawalGas',
						link: '/op-stack/actions/estimateProveWithdrawalGas',
					},
					{
						text: 'getL2Output',
						link: '/op-stack/actions/getL2Output',
					},
					{
						text: 'getTimeToFinalize',
						link: '/op-stack/actions/getTimeToFinalize',
					},
					{
						text: 'getTimeToNextL2Output',
						link: '/op-stack/actions/getTimeToNextL2Output',
					},
					{
						text: 'getTimeToProve',
						link: '/op-stack/actions/getTimeToProve',
					},
					{
						text: 'getWithdrawalStatus',
						link: '/op-stack/actions/getWithdrawalStatus',
					},
					{
						text: 'waitForNextL2Output',
						link: '/op-stack/actions/waitForNextL2Output',
					},
					{
						text: 'waitToFinalize',
						link: '/op-stack/actions/waitToFinalize',
					},
					{
						text: 'waitToProve',
						link: '/op-stack/actions/waitToProve',
					},
				],
			},
			{
				text: 'L1 Wallet Actions',
				items: [
					{
						text: 'depositTransaction',
						link: '/op-stack/actions/depositTransaction',
					},
					{
						text: 'finalizeWithdrawal',
						link: '/op-stack/actions/finalizeWithdrawal',
					},
					{
						text: 'proveWithdrawal',
						link: '/op-stack/actions/proveWithdrawal',
					},
				],
			},
			{
				text: 'Utilities',
				items: [
					{
						text: 'extractTransactionDepositedLogs',
						link: '/op-stack/utilities/extractTransactionDepositedLogs',
					},
					{
						text: 'extractWithdrawalMessageLogs',
						link: '/op-stack/utilities/extractWithdrawalMessageLogs',
					},
					{
						text: 'getL2TransactionHash',
						link: '/op-stack/utilities/getL2TransactionHash',
					},
					{
						text: 'getL2TransactionHashes',
						link: '/op-stack/utilities/getL2TransactionHashes',
					},
					{
						text: 'getWithdrawals',
						link: '/op-stack/utilities/getWithdrawals',
					},
					{
						text: 'getSourceHash',
						link: '/op-stack/utilities/getSourceHash',
					},
					{
						text: 'opaqueDataToDepositData',
						link: '/op-stack/utilities/opaqueDataToDepositData',
					},
					{
						text: 'getWithdrawalHashStorageSlot',
						link: '/op-stack/utilities/getWithdrawalHashStorageSlot',
					},
					{
						text: 'parseTransaction',
						link: '/op-stack/utilities/parseTransaction',
					},
					{
						text: 'serializeTransaction',
						link: '/op-stack/utilities/serializeTransaction',
					},
				],
			},
		],
	},
} satisfies Sidebar;
