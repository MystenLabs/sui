// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
	StandardConnectFeature,
	StandardDisconnectFeature,
	StandardEventsFeature,
	Wallet,
	WalletWithFeatures,
} from '@wallet-standard/core';
import { SuiFeatures } from './features';

export type StandardWalletAdapterWallet = WalletWithFeatures<
	StandardConnectFeature &
		StandardEventsFeature &
		SuiFeatures &
		// Disconnect is an optional feature:
		Partial<StandardDisconnectFeature>
>;

// These features are absolutely required for wallets to function in the Sui ecosystem.
// Eventually, as wallets have more consistent support of features, we may want to extend this list.
const REQUIRED_FEATURES: (keyof StandardWalletAdapterWallet['features'])[] = [
	'standard:connect',
	'standard:events',
];

export function isStandardWalletAdapterCompatibleWallet(
	wallet: Wallet,
	features: string[] = [],
): wallet is StandardWalletAdapterWallet {
	return [...REQUIRED_FEATURES, ...features].every((feature) => feature in wallet.features);
}
