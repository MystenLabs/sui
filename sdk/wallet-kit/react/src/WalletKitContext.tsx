// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
	createWalletKitCore,
	WalletKitCore,
	WalletKitCoreOptions,
	WalletKitCoreState,
} from '@mysten/wallet-kit-core';
import {
	createContext,
	ReactNode,
	useContext,
	useEffect,
	useMemo,
	useRef,
	useSyncExternalStore,
} from 'react';

import { registerUnsafeBurnerWallet } from './UnsafeBurnerWallet';

export const WalletKitContext = createContext<WalletKitCore | null>(null);

interface WalletKitProviderProps extends Partial<WalletKitCoreOptions> {
	/** Enable the development-only unsafe burner wallet, which is can be useful for testing. */
	enableUnsafeBurner?: boolean;
	children: ReactNode;
	disableAutoConnect?: boolean;
	// Define the wallet standard features that you will use. This will filter the list of wallets
	// displayed to the user.
	features?: string[];
}

export function WalletKitProvider({
	preferredWallets,
	children,
	enableUnsafeBurner,
	storageAdapter,
	storageKey,
	disableAutoConnect,
	features,
}: WalletKitProviderProps) {
	const walletKitRef = useRef<WalletKitCore | null>(null);
	if (!walletKitRef.current) {
		walletKitRef.current = createWalletKitCore({
			features,
			preferredWallets,
			storageAdapter,
			storageKey,
		});
	}

	useEffect(() => {
		if (!enableUnsafeBurner) return;

		return registerUnsafeBurnerWallet();
	}, [enableUnsafeBurner]);

	// Automatically trigger the autoconnect logic when we mount, and whenever wallets change:
	const { wallets } = useSyncExternalStore(
		walletKitRef.current.subscribe,
		walletKitRef.current.getState,
		walletKitRef.current.getState,
	);
	useEffect(() => {
		if (!disableAutoConnect) {
			walletKitRef.current?.autoconnect();
		}
		// eslint-disable-next-line react-hooks/exhaustive-deps
	}, [wallets]);

	return (
		<WalletKitContext.Provider value={walletKitRef.current}>{children}</WalletKitContext.Provider>
	);
}

type UseWalletKit = WalletKitCoreState &
	Pick<
		WalletKitCore,
		| 'connect'
		| 'disconnect'
		| 'selectAccount'
		| 'signMessage'
		| 'signPersonalMessage'
		| 'signTransactionBlock'
		| 'signAndExecuteTransactionBlock'
	>;

export function useWalletKit(): UseWalletKit {
	const walletKit = useContext(WalletKitContext);

	if (!walletKit) {
		throw new Error('You must call `useWalletKit` within the of the `WalletKitProvider`.');
	}

	const state = useSyncExternalStore(walletKit.subscribe, walletKit.getState, walletKit.getState);

	return useMemo(
		() => ({
			connect: walletKit.connect,
			disconnect: walletKit.disconnect,
			signMessage: walletKit.signMessage,
			signPersonalMessage: walletKit.signPersonalMessage,
			signTransactionBlock: walletKit.signTransactionBlock,
			signAndExecuteTransactionBlock: walletKit.signAndExecuteTransactionBlock,
			selectAccount: walletKit.selectAccount,
			...state,
		}),
		[walletKit, state],
	);
}
