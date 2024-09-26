// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SuiSignPersonalMessageInput } from '@mysten/wallet-standard';

import {
	WalletFeatureNotSupportedError,
	WalletNoAccountSelectedError,
	WalletNotConnectedError,
} from '../../errors/walletErrors.js';
import type { PartialBy } from '../../types/utilityTypes.js';
import type { StoreState } from '../../walletStore.js';
import { getCurrentAccount } from './getCurrentAccount.js';
import { getCurrentWallet } from './getCurrentWallet.js';

type UseSignPersonalMessageArgs = PartialBy<SuiSignPersonalMessageInput, 'account'>;

/**
 * Mutation hook for prompting the user to sign a message.
 */
export function getSignPersonalMessage(state: StoreState) {
	const { currentWallet } = getCurrentWallet(state);
	const currentAccount = getCurrentAccount(state);

	return async (signPersonalMessageArgs: UseSignPersonalMessageArgs) => {
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
	};
}
