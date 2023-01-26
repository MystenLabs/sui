// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { GrowthBook } from '@growthbook/growthbook';
import Browser from 'webextension-polyfill';

export const growthbook = new GrowthBook();

const WALLET_VERSION = Browser.runtime.getManifest().version;

export function setAttributes(network?: string | null) {
    growthbook.setAttributes({
        network,
        version: WALLET_VERSION,
        beta: process.env.WALLET_BETA || false,
    });
}
