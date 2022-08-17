// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isBasePayload } from '_payloads';

import type { SignaturePubkeyPair } from '@mysten/sui.js';
import type { BasePayload, Payload } from '_payloads';

export interface ExecuteSignMessageResponse extends BasePayload {
    type: 'execute-sign-message-response';
    signature?: SignaturePubkeyPair;
}

export function isExecuteSignMessageResponse(
    payload: Payload
): payload is ExecuteSignMessageResponse {
    return (
        isBasePayload(payload) &&
        payload.type === 'execute-sign-message-response'
    );
}
