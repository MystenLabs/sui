// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isBasePayload } from '_payloads';

import type { SuiAddress } from '@mysten/sui.js/src';
import type { BasePayload, Payload } from '_payloads';
import type { NetworkEnvType } from '_src/background/NetworkEnv';

export type WalletStatusChange = {
    network?: NetworkEnvType;
    accounts?: SuiAddress[];
};

export interface WalletStatusChangePayload
    extends BasePayload,
        WalletStatusChange {
    type: 'wallet-status-changed';
}

export function isWalletStatusChangePayload(
    payload: Payload
): payload is WalletStatusChangePayload {
    return isBasePayload(payload) && payload.type === 'wallet-status-changed';
}
