// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { WalletWithRequiredFeatures } from '@mysten/wallet-standard';
import { useUnsafeBurnerWallet } from '../hooks/wallet/useUnsafeBurnerWallet.js';
import { useWalletsChanged } from '../hooks/wallet/useWalletsChanged.js';
import type { ReactNode } from 'react';
import { useAutoConnectWallet } from '../hooks/wallet/useAutoConnectWallet.js';

type WalletConnectionManagerProps = {
	preferredWallets: string[];
	requiredFeatures: (keyof WalletWithRequiredFeatures['features'])[];
	enableUnsafeBurner: boolean;
	autoConnect: boolean;
	children: ReactNode;
};

export function WalletConnectionManager({
	preferredWallets,
	requiredFeatures,
	enableUnsafeBurner,
	autoConnect,
	children,
}: WalletConnectionManagerProps) {
	useWalletsChanged(preferredWallets, requiredFeatures);
	useUnsafeBurnerWallet(enableUnsafeBurner);
	useAutoConnectWallet(autoConnect);

	return children;
}
