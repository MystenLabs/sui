// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isBasePayload } from '_payloads';

import type { BasePayload, Payload } from '_payloads';
import type { SerializedSignaturePubkeyPair } from '_shared/signature-serialization';

export interface ExecuteSignMessageResponse extends BasePayload {
    type: 'execute-sign-message-response';
    signature?: SerializedSignaturePubkeyPair;
}

export function isExecuteSignMessageResponse(
    payload: Payload
): payload is ExecuteSignMessageResponse {
    return (
        isBasePayload(payload) &&
        payload.type === 'execute-sign-message-response'
    );
}
