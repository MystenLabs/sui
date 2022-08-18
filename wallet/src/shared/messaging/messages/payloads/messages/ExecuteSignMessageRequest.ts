// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isBasePayload } from '_payloads';

import type { BasePayload, Payload } from '_payloads';

export interface ExecuteSignMessageRequest extends BasePayload {
    type: 'execute-sign-message-request';
    messageData?: string; // base64 encoded string
    messageString?: string;
}

export function isExecuteSignMessageRequest(
    payload: Payload
): payload is ExecuteSignMessageRequest {
    return (
        isBasePayload(payload) &&
        payload.type === 'execute-sign-message-request'
    );
}
