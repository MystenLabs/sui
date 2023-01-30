// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import Browser from 'webextension-polyfill';

import { API_ENV } from '_src/ui/app/ApiProvider';

import type { GrowthBook } from '@growthbook/growthbook';

/**
 * This is a list of feature keys that are used in wallet
 * https://docs.growthbook.io/app/features#feature-keys
 */
export enum FEATURES {
    USE_LOCAL_TXN_SERIALIZER = 'use-local-txn-serializer',
    USE_TEST_NET_ENDPOINT = 'testnet-selection',
    STAKING_ENABLED = 'wallet-staking-enabled',
    WALLET_DAPPS = 'wallet-dapps',
}

export function setAttributes(
    growthBook: GrowthBook,
    network?: { apiEnv: API_ENV; customRPC?: string | null }
) {
    const activeNetwork = network
        ? network.apiEnv === API_ENV.customRPC && network.customRPC
            ? network.customRPC
            : network.apiEnv.toUpperCase()
        : null;
    growthBook.setAttributes({
        network: activeNetwork,
        version: Browser.runtime.getManifest().version,
        beta: process.env.WALLET_BETA || false,
    });
}
