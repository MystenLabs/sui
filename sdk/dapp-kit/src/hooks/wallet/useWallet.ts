// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useWalletContext } from 'dapp-kit/src/components/wallet-provider/WalletProvider';

export function useWallet() {
	const { wallets, currentWallet, accounts, currentAccount } = useWalletContext();
	return { wallets, currentWallet, accounts, currentAccount };
}
