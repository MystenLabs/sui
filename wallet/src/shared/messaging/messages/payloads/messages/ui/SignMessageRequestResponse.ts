// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isBasePayload } from '_payloads';

import type { BasePayload, Payload } from '_payloads';
import type { SerializedSignaturePubkeyPair } from '_shared/signature-serialization';

export interface SignMessageRequestResponse extends BasePayload {
    type: 'sign-message-request-response';
    signMessageRequestID: string;
    approved: boolean;
    signature?: SerializedSignaturePubkeyPair;
    error?: string;
}

export function isSignMessageRequestResponse(
    payload: Payload
): payload is SignMessageRequestResponse {
    return (
        isBasePayload(payload) &&
        payload.type === 'sign-message-request-response'
    );
}
