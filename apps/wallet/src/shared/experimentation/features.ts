// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { GrowthBook } from '@growthbook/growthbook';
import Browser from 'webextension-polyfill';

import { API_ENV } from '_src/shared/api-env';

const GROWTHBOOK_API_KEY =
    process.env.NODE_ENV === 'production'
        ? 'sdk-lJ5zaQ6WI9uPth6C'
        : 'sdk-iUMYcob41m3pnAK';

export const growthbook = new GrowthBook({
    apiHost: 'https://cdn.growthbook.io',
    clientKey: GROWTHBOOK_API_KEY,
    enableDevMode: true,
});

/**
 * This is a list of feature keys that are used in wallet
 * https://docs.growthbook.io/app/features#feature-keys
 */
export enum FEATURES {
    USE_LOCAL_TXN_SERIALIZER = 'use-local-txn-serializer',
    USE_TEST_NET_ENDPOINT = 'testnet-selection',
    USE_MAINNET_ENDPOINT = 'mainnet-selection',
    STAKING_ENABLED = 'wallet-staking-enabled',
    WALLET_DAPPS = 'wallet-dapps',
    WALLET_MULTI_ACCOUNTS = 'wallet-multi-accounts',
    WALLET_LEDGER_INTEGRATION = 'wallet-ledger-integration',
    WALLET_BALANCE_REFETCH_INTERVAL = 'wallet-balance-refetch-interval',
    WALLET_ACTIVITY_REFETCH_INTERVAL = 'wallet-activity-refetch-interval',
    WALLET_EFFECTS_ONLY_SHARED_TRANSACTION = 'wallet-effects-only-shared-transaction',
}

export function setAttributes(network?: {
    apiEnv: API_ENV;
    customRPC?: string | null;
}) {
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
