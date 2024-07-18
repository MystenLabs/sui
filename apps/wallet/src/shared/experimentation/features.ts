// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { API_ENV } from '_src/shared/api-env';
import { GrowthBook } from '@growthbook/growthbook';
import Browser from 'webextension-polyfill';

export const growthbook = new GrowthBook({
	// If you want to develop locally, you can set the API host to this:
	// apiHost: 'http://localhost:3003',
	apiHost: 'https://apps-backend.sui.io',
	clientKey: process.env.NODE_ENV === 'development' ? 'development' : 'production',
	enableDevMode: process.env.NODE_ENV === 'development',
});

/**
 * This is a list of feature keys that are used in wallet
 * https://docs.growthbook.io/app/features#feature-keys
 */
export enum FEATURES {
	USE_LOCAL_TXN_SERIALIZER = 'use-local-txn-serializer',
	WALLET_DAPPS = 'wallet-dapps',
	WALLET_BALANCE_REFETCH_INTERVAL = 'wallet-balance-refetch-interval',
	WALLET_ACTIVITY_REFETCH_INTERVAL = 'wallet-activity-refetch-interval',
	WALLET_EFFECTS_ONLY_SHARED_TRANSACTION = 'wallet-effects-only-shared-transaction',
	WALLET_QREDO = 'wallet-qredo',
	WALLET_APPS_BANNER_CONFIG = 'wallet-apps-banner-config',
	WALLET_INTERSTITIAL_CONFIG = 'wallet-interstitial-config',
	WALLET_DEFI = 'wallet-defi',
	WALLET_FEE_ADDRESS = 'wallet-fee-address',
	DEEP_BOOK_CONFIGS = 'deep-book-configs',
	TOKEN_METADATA_OVERRIDES = 'token-metadata-overrides',
}

export function setAttributes(network?: { apiEnv: API_ENV; customRPC?: string | null }) {
	const activeNetwork = network
		? network.apiEnv === API_ENV.customRPC && network.customRPC
			? network.customRPC
			: network.apiEnv.toUpperCase()
		: null;

	growthbook.setAttributes({
		network: activeNetwork,
		version: Browser.runtime.getManifest().version,
		beta: process.env.WALLET_BETA || false,
	});
}

// Initialize growthbook to default attributes:
setAttributes();
