// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SuiClient } from '@mysten/sui/client';
import type { Signer } from '@mysten/sui/cryptography';
import { Transaction } from '@mysten/sui/transactions';
import { toBase64 } from '@mysten/sui/utils';
import type {
	StandardConnectFeature,
	StandardConnectMethod,
	StandardEventsFeature,
	StandardEventsOnMethod,
	SuiFeatures,
	SuiSignAndExecuteTransactionMethod,
	SuiSignPersonalMessageMethod,
	SuiSignTransactionMethod,
	Wallet,
	WalletIcon,
} from '@mysten/wallet-standard';
import { getWallets, ReadonlyWalletAccount, SUI_CHAINS } from '@mysten/wallet-standard';
import { useEffect } from 'react';

import { useSuiClient } from '../useSuiClient.js';

const WALLET_NAME = 'Unsafe Burner Wallet';

export interface SignerWalletOptions {
	name: string;
	signer: Signer;
	icon: WalletIcon;
}

export function useSignerWallet({
	signer,
	name,
	icon,
}: Omit<SignerWalletOptions, 'signer'> & { signer?: Signer | null }) {
	const suiClient = useSuiClient();

	useEffect(() => {
		if (!signer) {
			return;
		}
		const unregister = registerSignerWallet(suiClient, {
			name,
			signer,
			icon,
		});
		return unregister;
	}, [signer, name, icon, suiClient]);
}

function registerSignerWallet(suiClient: SuiClient, { name, signer, icon }: SignerWalletOptions) {
	const walletsApi = getWallets();
	const registeredWallets = walletsApi.get();

	if (registeredWallets.find((wallet) => wallet.name === name)) {
		console.warn(
			`registerSignerWallet: Wallet with name ${name} already registered, skipping duplicate registration.`,
		);
		return;
	}

	const publicKey = signer.getPublicKey();

	const account = new ReadonlyWalletAccount({
		address: publicKey.toSuiAddress(),
		publicKey: publicKey.toSuiBytes(),
		chains: ['sui:unknown'],
		features: [
			'sui:signAndExecuteTransactionBlock',
			'sui:signTransactionBlock',
			'sui:signTransaction',
			'sui:signAndExecuteTransaction',
		],
	});

	class SignerWallet implements Wallet {
		get version() {
			return '1.0.0' as const;
		}

		get name() {
			return WALLET_NAME;
		}

		get icon() {
			return icon;
		}

		// Return the Sui chains that your wallet supports.
		get chains() {
			return SUI_CHAINS;
		}

		get accounts() {
			return [account];
		}

		get features(): StandardConnectFeature & StandardEventsFeature & SuiFeatures {
			return {
				'standard:connect': {
					version: '1.0.0',
					connect: this.#connect,
				},
				'standard:events': {
					version: '1.0.0',
					on: this.#on,
				},
				'sui:signPersonalMessage': {
					version: '1.0.0',
					signPersonalMessage: this.#signPersonalMessage,
				},
				'sui:signTransaction': {
					version: '2.0.0',
					signTransaction: this.#signTransaction,
				},
				'sui:signAndExecuteTransaction': {
					version: '2.0.0',
					signAndExecuteTransaction: this.#signAndExecuteTransaction,
				},
			};
		}

		#on: StandardEventsOnMethod = () => {
			return () => {};
		};

		#connect: StandardConnectMethod = async () => {
			return { accounts: this.accounts };
		};

		#signPersonalMessage: SuiSignPersonalMessageMethod = async (messageInput) => {
			const { bytes, signature } = await signer.signPersonalMessage(messageInput.message);
			return { bytes, signature };
		};

		#signTransaction: SuiSignTransactionMethod = async (transactionInput) => {
			const { bytes, signature } = await Transaction.from(
				await transactionInput.transaction.toJSON(),
			).sign({
				client: suiClient,
				signer,
			});

			transactionInput.signal?.throwIfAborted();

			return {
				bytes,
				signature: signature,
			};
		};

		#signAndExecuteTransaction: SuiSignAndExecuteTransactionMethod = async (transactionInput) => {
			const { bytes, signature } = await Transaction.from(
				await transactionInput.transaction.toJSON(),
			).sign({
				client: suiClient,
				signer,
			});

			transactionInput.signal?.throwIfAborted();

			const { rawEffects, digest } = await suiClient.executeTransactionBlock({
				signature,
				transactionBlock: bytes,
				options: {
					showRawEffects: true,
				},
			});

			return {
				bytes,
				signature,
				digest,
				effects: toBase64(new Uint8Array(rawEffects!)),
			};
		};
	}

	return walletsApi.register(new SignerWallet());
}
