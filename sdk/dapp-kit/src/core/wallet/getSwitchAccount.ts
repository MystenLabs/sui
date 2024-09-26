// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { WalletAccount } from '@mysten/wallet-standard';

import { WalletAccountNotFoundError, WalletNotConnectedError } from '../../errors/walletErrors.js';
import type { StoreState } from '../../walletStore.js';
import { getCurrentWallet } from './getCurrentWallet.js';

type SwitchAccountArgs = {
	account: WalletAccount;
};

/**
 * Mutation hook for switching to a specific wallet account.
 */
export function getSwitchAccount(state: StoreState) {
	const { setAccountSwitched } = state;
	const { currentWallet } = getCurrentWallet(state);

	return async ({ account }: SwitchAccountArgs) => {
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

		setAccountSwitched(accountToSelect);
	};
}
