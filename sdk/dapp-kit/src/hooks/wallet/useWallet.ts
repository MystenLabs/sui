// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useWalletContext } from 'dapp-kit/src/components/wallet-provider/WalletProvider';
import { useMemo } from 'react';

/**
 * Hook for retrieving wallet and account information.
 */
export function useWallet() {
	const { wallets, currentWallet, accounts, currentAccount } = useWalletContext();
	return useMemo(
		() => ({ wallets, currentWallet, accounts, currentAccount }),
		[accounts, currentAccount, currentWallet, wallets],
	);
}
