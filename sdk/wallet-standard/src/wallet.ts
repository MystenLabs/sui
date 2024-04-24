// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { toB64 } from '@mysten/sui.js/utils';
import type { Wallet, WalletWithFeatures } from '@wallet-standard/core';
import { getWallets } from '@wallet-standard/core';

import { isWalletWithRequiredFeatureSet } from './detect.js';

import './features/index.js';

import { TransactionBlock } from '@mysten/sui.js/transactions';

import type { MinimallyRequiredFeatures, SuiWalletFeatures } from './features/index.js';

declare module '@wallet-standard/core' {
	export interface Wallet {
		/**
		 * Unique identifier of the Wallet.
		 *
		 * If not provided, the wallet name will be used as the identifier.
		 */
		readonly id?: string;
	}

	export interface StandardConnectOutput {
		supportedIntents?: string[];
	}
}

export type { Wallet } from '@wallet-standard/core';

export function getNormalizedSuiWallets<
	AdditionalFeatures extends keyof KnownFeatures & keyof Wallet['features'] = never,
	KnownFeatures extends Wallet['features'] = SuiWalletFeatures,
>(
	features: AdditionalFeatures[] = [],
): WalletWithFeatures<Pick<KnownFeatures, AdditionalFeatures | keyof MinimallyRequiredFeatures>>[] {
	const wallets = getWallets().get();

	return wallets
		.map((wallet) => normalizeWalletFeatures(wallet))
		.filter((wallet) => isWalletWithRequiredFeatureSet(wallet, features)) as WalletWithFeatures<
		Pick<KnownFeatures, AdditionalFeatures | keyof MinimallyRequiredFeatures>
	>[];
}

function normalizeWalletFeatures(wallet: WalletWithFeatures<Partial<SuiWalletFeatures>>) {
	const features = {
		...wallet.features,
	};

	if (
		wallet.features['sui:signTransactionBlock'] &&
		!wallet.features['sui:signTransactionBlock:v2']
	) {
		const { signTransactionBlock } = wallet.features['sui:signTransactionBlock'];
		features['sui:signTransactionBlock:v2'] = {
			version: '2.0.0',
			signTransactionBlock: async (input) => {
				const transactionBlock = TransactionBlock.from(input.transactionBlock);
				const { transactionBlockBytes, signature } = await signTransactionBlock({
					...input,
					transactionBlock,
				});

				return { transactionBlockBytes, signature };
			},
		};
	}

	if (
		wallet.features['sui:signAndExecuteTransactionBlock'] &&
		!wallet.features['sui:signAndExecuteTransactionBlock:v2']
	) {
		const { signAndExecuteTransactionBlock } =
			wallet.features['sui:signAndExecuteTransactionBlock'];
		features['sui:signAndExecuteTransactionBlock:v2'] = {
			version: '2.0.0',
			signAndExecuteTransactionBlock: async (input) => {
				const transactionBlock = TransactionBlock.from(input.transactionBlock);
				const { rawEffects, balanceChanges } = await signAndExecuteTransactionBlock({
					...input,
					transactionBlock,
					options: {
						showRawEffects: true,
						showBalanceChanges: true,
					},
				});

				return {
					effects: rawEffects ? toB64(new Uint8Array(rawEffects)) : null,
					balanceChanges:
						balanceChanges?.map(({ coinType, amount, owner }) => {
							const address =
								(owner as Extract<typeof owner, { AddressOwner: unknown }>).AddressOwner ??
								(owner as Extract<typeof owner, { ObjectOwner: unknown }>).ObjectOwner;

							return {
								coinType,
								amount,
								address,
							};
						}) ?? null,
				};
			},
		};
	}

	return {
		...wallet,
		features,
	};
}
