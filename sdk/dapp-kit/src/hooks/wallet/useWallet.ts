// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export function useWallet(): UseWallet {
	const wallet = useContext(WalletContext);

	if (!wallet) {
		throw new Error('You must call `useWallet` within the of the `WalletProvider`.');
	}

	const state = useSyncExternalStore(wallet.subscribe, wallet.getState, wallet.getState);

	return useMemo(
		() => ({
			connect: wallet.connect,
			disconnect: wallet.disconnect,
			signMessage: wallet.signMessage,
			signPersonalMessage: wallet.signPersonalMessage,
			signTransactionBlock: wallet.signTransactionBlock,
			signAndExecuteTransactionBlock: wallet.signAndExecuteTransactionBlock,
			selectAccount: wallet.selectAccount,
			...state,
		}),
		[wallet, state],
	);
}
