// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { bcs } from '@mysten/sui.js/bcs';
import { TransactionBlock } from '@mysten/sui.js/transactions';
import { fromB64, toB64 } from '@mysten/sui.js/utils';
import type { Wallet, WalletWithFeatures } from '@wallet-standard/core';
import { getWallets } from '@wallet-standard/core';

import { isWalletWithRequiredFeatureSet } from './detect.js';
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
			signTransactionBlock: () => async (input) => {
				const transactionBlock = TransactionBlock.from(input.transactionBlock);
				const { transactionBlockBytes, signature } = await signTransactionBlock({
					...input,
					transactionBlock,
				});

				return { bytes: transactionBlockBytes, signature };
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
			signAndExecuteTransactionBlock: () => async (input) => {
				const transactionBlock = TransactionBlock.from(input.transactionBlock);
				const { digest, rawEffects, balanceChanges, rawTransaction } =
					await signAndExecuteTransactionBlock({
						...input,
						transactionBlock,
						options: {
							showRawEffects: true,
							showBalanceChanges: true,
							showRawInput: true,
						},
					});

				const [
					{
						txSignatures: [signature],
						intentMessage: { value: bcsTransaction },
					},
				] = bcs.SenderSignedData.parse(fromB64(rawTransaction!));

				const bytes = bcs.TransactionData.serialize(bcsTransaction).toBase64();

				return {
					digest,
					signature,
					bytes,
					effects: toB64(new Uint8Array(rawEffects!)),
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
		get id() {
			return wallet.id;
		},
		get name() {
			return wallet.name;
		},
		get version() {
			return wallet.version;
		},
		get icon() {
			return wallet.icon;
		},
		get chains() {
			return wallet.chains;
		},
		get features() {
			return features;
		},
		get accounts() {
			return wallet.accounts;
		},
	};
}
