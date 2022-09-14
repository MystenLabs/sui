// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Network } from './api/rpcSetting';

const HOST_TO_NETWORK: Record<string, Network> = {
    ci: Network.CI,
    staging: Network.Staging,
    devnet: Network.Devnet,
    static: Network.Static,
};

export let CURRENT_ENV: Network = Network.Local;
if (import.meta.env.VITE_NETWORK) {
    CURRENT_ENV = HOST_TO_NETWORK[import.meta.env.VITE_NETWORK];
} else if (
    typeof window !== 'undefined' &&
    window.location.hostname.includes('.sui.io')
) {
    const host = window.location.hostname.split('.').at(-3) || 'devnet';
    CURRENT_ENV = HOST_TO_NETWORK[host] || Network.Devnet;
}

export const IS_STATIC_ENV = CURRENT_ENV === Network.Static;
export const IS_STAGING_ENV = CURRENT_ENV === Network.Staging;
