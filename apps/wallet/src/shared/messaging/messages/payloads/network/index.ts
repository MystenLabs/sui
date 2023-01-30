// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isBasePayload } from '_payloads';

import type { BasePayload, Payload } from '_payloads';
import type { NetworkEnvType } from '_src/background/NetworkEnv';

export interface SetNetworkPayload extends BasePayload {
    type: 'set-network';
    network: NetworkEnvType;
}

export function isSetNetworkPayload(
    payload: Payload
): payload is SetNetworkPayload {
    return isBasePayload(payload) && payload.type === 'set-network';
}
