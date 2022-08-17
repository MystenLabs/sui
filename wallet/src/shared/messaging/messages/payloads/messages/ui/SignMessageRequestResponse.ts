// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isBasePayload } from '_payloads';

import type { SignaturePubkeyPair } from '@mysten/sui.js';
import type { BasePayload, Payload } from '_payloads';

export interface SignMessageRequestResponse extends BasePayload {
    type: 'sign-message-request-response';
    signMessageRequestID: string;
    approved: boolean;
    signature?: SignaturePubkeyPair;
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
