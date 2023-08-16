// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Wallet } from '@wallet-standard/core';
import { WalletWithSuiFeatures } from './features';

// These features are absolutely required for wallets to function in the Sui ecosystem.
// Eventually, as wallets have more consistent support of features, we may want to extend this list.
const REQUIRED_FEATURES: (keyof WalletWithSuiFeatures['features'])[] = [
	'standard:connect',
	'standard:events',
];

export function isWalletWithSuiFeatures(
	wallet: Wallet,
	/** Extra features that are required to be present, in addition to the expected feature set. */
	features: string[] = [],
): wallet is WalletWithSuiFeatures {
	return [...REQUIRED_FEATURES, ...features].every((feature) => feature in wallet.features);
}
