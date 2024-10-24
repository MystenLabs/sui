// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SuiClient } from '@mysten/sui/client';
import type { Transaction } from '@mysten/sui/src/transactions';
import { toB64 } from '@mysten/sui/utils';
import { signTransaction } from '@mysten/wallet-standard';
import type {
	SignedTransaction,
	StandardConnectInput,
	StandardConnectOutput,
	SuiReportTransactionEffectsInput,
	SuiSignAndExecuteTransactionInput,
	SuiSignAndExecuteTransactionOutput,
	SuiSignPersonalMessageInput,
	SuiSignPersonalMessageOutput,
	SuiSignTransactionInput,
	WalletAccount,
	WalletWithRequiredFeatures,
} from '@mysten/wallet-standard';
import type { ReadableAtom } from 'nanostores';
import { task } from 'nanostores';

import {
	WalletAccountNotFoundError,
	WalletFeatureNotSupportedError,
	WalletNoAccountSelectedError,
	WalletNotConnectedError,
} from '../../errors/walletErrors.js';
import type { PartialBy } from '../../types/utilityTypes.js';
import type { createState } from './state.js';

export type Methods = {
	switchAccount: (input: { account: WalletAccount }) => void;

	connectWallet: (
		input: {
			/** The wallet to connect to. */
			wallet: WalletWithRequiredFeatures;

			/** An optional account address to connect to. Defaults to the first authorized account. */
			accountAddress?: string;
		} & StandardConnectInput,
	) => Promise<StandardConnectOutput>;

	disconnectWallet: () => void;

	reportTransactionEffects: (
		input: Omit<PartialBy<SuiReportTransactionEffectsInput, 'account' | 'chain'>, 'effects'> & {
			effects: string | number[];
		},
	) => Promise<void>;

	signTransaction: (
		input: PartialBy<Omit<SuiSignTransactionInput, 'transaction'>, 'account' | 'chain'> & {
			transaction: Transaction | string;
		},
	) => Promise<
		SignedTransaction & {
			reportTransactionEffects: (effects: string) => void;
		}
	>;

	signPersonalMessage: (
		input: PartialBy<SuiSignPersonalMessageInput, 'account'>,
	) => Promise<SuiSignPersonalMessageOutput>;

	// signAndExecuteTransaction: <
	// 	Result extends ExecuteTransactionResult = SuiSignAndExecuteTransactionOutput,
	// >(
	// 	input: PartialBy<
	// 		Omit<SuiSignAndExecuteTransactionInput, 'transaction'>,
	// 		'account' | 'chain'
	// 	> & {
	// 		transaction: Transaction | string;
	// 		execute?: ({ bytes, signature }: { bytes: string; signature: string }) => Promise<Result>;
	// 	},
	// ) => Promise<SuiSignAndExecuteTransactionOutput>;
	signAndExecuteTransaction: (
		input: PartialBy<
			Omit<SuiSignAndExecuteTransactionInput, 'transaction'>,
			'account' | 'chain'
		> & {
			transaction: Transaction | string;
			execute?: ({
				bytes,
				signature,
			}: {
				bytes: string;
				signature: string;
			}) => Promise<SuiSignAndExecuteTransactionOutput>;
		},
	) => Promise<SuiSignAndExecuteTransactionOutput>;
};

type ExecuteTransactionResult =
	| {
			digest: string;
			rawEffects?: number[];
	  }
	| {
			effects?: {
				bcs?: string;
			};
	  };

export type MethodTypes = {
	[p in keyof Methods]: {
		input: Parameters<Methods[p]>[0];
		output: ReturnType<Methods[p]>;
	};
};

export function createMethods({
	$state,
	actions,
	$client,
}: Pick<ReturnType<typeof createState>, '$state' | 'actions'> & {
	$client: ReadableAtom<SuiClient>;
}) {
	const methods = {
		switchAccount({ account }) {
			const { currentWallet } = $state.get();
			if (!currentWallet) {
				throw new WalletNotConnectedError('No wallet is connected.');
			}

			const accountToSelect = currentWallet.accounts.find(
				(walletAccount) => walletAccount.address === account.address,
			);

			if (!accountToSelect) {
				throw new WalletAccountNotFoundError(
					`No account with address ${account.address} is connected to ${currentWallet.name}.`,
				);
			}

			actions.setAccountSwitched(accountToSelect);
		},
		connectWallet({ wallet, accountAddress, ...connectArgs }) {
			return task(async () => {
				try {
					actions.setConnectionStatus('connecting');

					const connectResult = await wallet.features['standard:connect'].connect(connectArgs);
					const connectedSuiAccounts = connectResult.accounts.filter((account) =>
						account.chains.some((chain) => chain.split(':')[0] === 'sui'),
					);
					const selectedAccount = getSelectedAccount(connectedSuiAccounts, accountAddress);

					actions.setWalletConnected(
						wallet,
						connectedSuiAccounts,
						selectedAccount,
						connectResult.supportedIntents,
					);

					return { accounts: connectedSuiAccounts };
				} catch (error) {
					actions.setConnectionStatus('disconnected');
					throw error;
				}
			});
		},
		disconnectWallet() {
			return task(async () => {
				const { currentWallet } = $state.get();

				if (!currentWallet) {
					throw new WalletNotConnectedError('No wallet is connected.');
				}

				try {
					// Wallets aren't required to implement the disconnect feature, so we'll
					// optionally call the disconnect feature if it exists and reset the UI
					// state on the frontend at a minimum.
					await currentWallet.features['standard:disconnect']?.disconnect();
				} catch (error) {
					console.error('Failed to disconnect the application from the current wallet.', error);
				}

				actions.setWalletDisconnected();
			});
		},
		reportTransactionEffects({ effects, chain, account }) {
			return task(async () => {
				const { currentWallet, currentAccount } = $state.get();

				chain = chain ?? currentWallet?.chains[0];
				account = account ?? currentAccount ?? undefined;

				if (!currentWallet) {
					throw new WalletNotConnectedError('No wallet is connected.');
				}

				if (!account) {
					throw new WalletNoAccountSelectedError(
						'No wallet account is selected to report transaction effects for',
					);
				}

				const reportTransactionEffectsFeature =
					currentWallet.features['sui:reportTransactionEffects'];

				if (reportTransactionEffectsFeature) {
					return await reportTransactionEffectsFeature.reportTransactionEffects({
						effects: Array.isArray(effects) ? toB64(new Uint8Array(effects)) : effects,
						account,
						chain: chain ?? currentWallet?.chains[0],
					});
				}
			});
		},
		signTransaction({ transaction, ...signTransactionArgs }) {
			return task(async () => {
				const client = $client.get();
				const { currentWallet, currentAccount } = $state.get();
				if (!currentWallet) {
					throw new WalletNotConnectedError('No wallet is connected.');
				}

				const signerAccount = signTransactionArgs.account ?? currentAccount;
				if (!signerAccount) {
					throw new WalletNoAccountSelectedError(
						'No wallet account is selected to sign the transaction with.',
					);
				}

				if (
					!currentWallet.features['sui:signTransaction'] &&
					!currentWallet.features['sui:signTransactionBlock']
				) {
					throw new WalletFeatureNotSupportedError(
						"This wallet doesn't support the `signTransaction` feature.",
					);
				}

				const { bytes, signature } = await signTransaction(currentWallet, {
					...signTransactionArgs,
					transaction: {
						toJSON: async () => {
							return typeof transaction === 'string'
								? transaction
								: await transaction.toJSON({
										supportedIntents: [],
										client,
									});
						},
					},
					account: signerAccount,
					chain: signTransactionArgs.chain ?? signerAccount.chains[0],
				});

				return {
					bytes,
					signature,
					reportTransactionEffects: (effects: string) => {
						methods.reportTransactionEffects({
							effects,
							account: signerAccount,
							chain: signTransactionArgs.chain ?? signerAccount.chains[0],
						});
					},
				};
			});
		},
		signPersonalMessage(signPersonalMessageArgs) {
			return task(async () => {
				const { currentWallet, currentAccount } = $state.get();

				if (!currentWallet) {
					throw new WalletNotConnectedError('No wallet is connected.');
				}

				const signerAccount = signPersonalMessageArgs.account ?? currentAccount;
				if (!signerAccount) {
					throw new WalletNoAccountSelectedError(
						'No wallet account is selected to sign the personal message with.',
					);
				}

				const signPersonalMessageFeature = currentWallet.features['sui:signPersonalMessage'];
				if (signPersonalMessageFeature) {
					return await signPersonalMessageFeature.signPersonalMessage({
						...signPersonalMessageArgs,
						account: signerAccount,
					});
				}

				// TODO: Remove this once we officially discontinue sui:signMessage in the wallet standard
				const signMessageFeature = currentWallet.features['sui:signMessage'];
				if (signMessageFeature) {
					console.warn(
						"This wallet doesn't support the `signPersonalMessage` feature... falling back to `signMessage`.",
					);

					const { messageBytes, signature } = await signMessageFeature.signMessage({
						...signPersonalMessageArgs,
						account: signerAccount,
					});
					return { bytes: messageBytes, signature };
				}

				throw new WalletFeatureNotSupportedError(
					"This wallet doesn't support the `signPersonalMessage` feature.",
				);
			});
		},
		signAndExecuteTransaction: <
			Result extends ExecuteTransactionResult = SuiSignAndExecuteTransactionOutput,
		>({
			transaction,
			execute,
			...signTransactionArgs
		}: PartialBy<Omit<SuiSignAndExecuteTransactionInput, 'transaction'>, 'account' | 'chain'> & {
			transaction: Transaction | string;
			execute?: ({ bytes, signature }: { bytes: string; signature: string }) => Promise<Result>;
		}) => {
			return task(async () => {
				const client = $client.get();
				const { currentWallet, currentAccount, supportedIntents } = $state.get();

				const executeTransaction: ({
					bytes,
					signature,
				}: {
					bytes: string;
					signature: string;
				}) => Promise<ExecuteTransactionResult> =
					execute ??
					(async ({ bytes, signature }) => {
						const { digest, rawEffects } = await client.executeTransactionBlock({
							transactionBlock: bytes,
							signature,
							options: {
								showRawEffects: true,
							},
						});

						return {
							digest,
							rawEffects,
							effects: toB64(new Uint8Array(rawEffects!)),
							bytes,
							signature,
						};
					});

				if (!currentWallet) {
					throw new WalletNotConnectedError('No wallet is connected.');
				}

				const signerAccount = signTransactionArgs.account ?? currentAccount;
				if (!signerAccount) {
					throw new WalletNoAccountSelectedError(
						'No wallet account is selected to sign the transaction with.',
					);
				}
				const chain = signTransactionArgs.chain ?? signerAccount?.chains[0];

				if (
					!currentWallet.features['sui:signTransaction'] &&
					!currentWallet.features['sui:signTransactionBlock']
				) {
					throw new WalletFeatureNotSupportedError(
						"This wallet doesn't support the `signTransaction` feature.",
					);
				}

				const { signature, bytes } = await signTransaction(currentWallet, {
					...signTransactionArgs,
					transaction: {
						async toJSON() {
							return typeof transaction === 'string'
								? transaction
								: await transaction.toJSON({
										supportedIntents,
										client,
									});
						},
					},
					account: signerAccount,
					chain: signTransactionArgs.chain ?? signerAccount.chains[0],
				});

				const result = await executeTransaction({ bytes, signature });

				let effects: string;

				if ('effects' in result && result.effects?.bcs) {
					effects = result.effects.bcs;
				} else if ('rawEffects' in result) {
					effects = toB64(new Uint8Array(result.rawEffects!));
				} else {
					throw new Error('Could not parse effects from transaction result.');
				}

				methods.reportTransactionEffects({ effects, account: signerAccount, chain });

				return result as Result;
			});
		},
	} satisfies Methods;

	return methods;
}

function getSelectedAccount(connectedAccounts: readonly WalletAccount[], accountAddress?: string) {
	if (connectedAccounts.length === 0) {
		return null;
	}

	if (accountAddress) {
		const selectedAccount = connectedAccounts.find((account) => account.address === accountAddress);
		return selectedAccount ?? connectedAccounts[0];
	}

	return connectedAccounts[0];
}
