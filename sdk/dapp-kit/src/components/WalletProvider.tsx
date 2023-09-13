// // Copyright (c) Mysten Labs, Inc.
// // SPDX-License-Identifier: Apache-2.0

// import type { ReactNode } from 'react';
// import { useCallback, useRef } from 'react';
// import type {
// 	MinimallyRequiredFeatures,
// 	Wallet,
// 	WalletWithFeatures,
// 	WalletWithRequiredFeatures,
// } from '@mysten/wallet-standard';
// import { getWallets, isWalletWithRequiredFeatureSet } from '@mysten/wallet-standard';
// import { useUnsafeBurnerWallet } from '../hooks/wallet/useUnsafeBurnerWallet.js';
// import { useWalletsChanged } from '../hooks/wallet/useWalletsChanged.js';

// import type { StateStorage } from 'zustand/middleware';

// export function WalletProvider({
// 	preferredWallets = [SUI_WALLET_NAME],
// 	requiredFeatures = [],
// 	storage,
// 	storageKey = DEFUALT_STORAGE_KEY,
// 	enableUnsafeBurner = false,
// 	autoConnect = false,
// 	children,
// }: WalletProviderProps) {
// 	const walletsApi = getWallets();
// 	const registeredWallets = sortWallets(walletsApi.get(), preferredWallets, requiredFeatures);

// 	const { setWalletRegistered, setWalletUnregistered } = walletStoreRef.current.getState();

// 	const onWalletRegistered = useCallback(() => {
// 		setWalletRegistered(sortWallets(walletsApi.get(), preferredWallets, requiredFeatures));
// 	}, [preferredWallets, requiredFeatures, setWalletRegistered, walletsApi]);

// 	const onWalletUnregistered = useCallback(
// 		(unregisteredWallet: Wallet) => {
// 			setWalletUnregistered(
// 				sortWallets(walletsApi.get(), preferredWallets, requiredFeatures),
// 				unregisteredWallet,
// 			);
// 		},
// 		[preferredWallets, requiredFeatures, setWalletUnregistered, walletsApi],
// 	);

// 	useWalletsChanged({
// 		onWalletRegistered,
// 		onWalletUnregistered,
// 	});

// 	useUnsafeBurnerWallet(enableUnsafeBurner);

// 	// useEffect(() => {
// 	// 	(async function autoConnectWallet() {
// 	// 		const connectionInfo = await getWalletConnectionInfo(storageAdapter, storageKey);
// 	// 		const { walletName, accountAddress } = connectionInfo || {};
// 	// 		const wallet = walletName ? wallets.find((wallet) => wallet.name === walletName) : null;

// 	// 		if (wallet) {
// 	// 			connectWallet({ wallet, accountAddress, silent: true });
// 	// 		}
// 	// 	})();
// 	// }, [connectWallet, storageAdapter, storageKey, wallets]);
// }

// function sortWallets<AdditionalFeatures extends Wallet['features']>(
// 	wallets: readonly Wallet[],
// 	preferredWallets: string[],
// 	requiredFeatures?: (keyof AdditionalFeatures)[],
// ) {
// 	const suiWallets = wallets.filter(
// 		(wallet): wallet is WalletWithFeatures<MinimallyRequiredFeatures & AdditionalFeatures> =>
// 			isWalletWithRequiredFeatureSet(wallet, requiredFeatures),
// 	);

// 	return [
// 		// Preferred wallets, in order:
// 		...(preferredWallets
// 			.map((name) => suiWallets.find((wallet) => wallet.name === name))
// 			.filter(Boolean) as WalletWithFeatures<MinimallyRequiredFeatures & AdditionalFeatures>[]),

// 		// Wallets in default order:
// 		...suiWallets.filter((wallet) => !preferredWallets.includes(wallet.name)),
// 	];
// }
