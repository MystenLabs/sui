// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Wallet } from '@wallet-standard/core';
import { WalletFeatureName, WalletWithSuiFeatures } from './features';

// These features are absolutely required for wallets to function in the Sui ecosystem.
// Eventually, as wallets have more consistent support of features, we may want to extend this list.
const REQUIRED_BASE_FEATURES = ['standard:connect', 'standard:events'] as const;

export type DefaultRequiredFeatureName = (typeof REQUIRED_BASE_FEATURES)[number];
export type AdditionallyRequiredWalletFeatureName = Exclude<
	WalletFeatureName,
	DefaultRequiredFeatureName
>;

export function isWalletWithSuiFeatures(
	wallet: Wallet,
	/** Extra features that are required to be present, in addition to the expected feature set. */
	additionalFeatures: AdditionallyRequiredWalletFeatureName[] = [],
): wallet is WalletWithSuiFeatures {
	return [...REQUIRED_BASE_FEATURES, ...additionalFeatures].every(
		(feature) => feature in wallet.features,
	);
}
