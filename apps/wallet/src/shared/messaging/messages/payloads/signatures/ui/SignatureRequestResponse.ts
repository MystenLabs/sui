// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isBasePayload } from '_payloads';

import type { SignaturePubkeyPair } from '@mysten/sui.js';
import type { BasePayload, Payload } from '_payloads';

export interface SignatureRequestResponse extends BasePayload {
    type: 'signature-request-response';
    sigId: string;
    signed: boolean;
    sigResult?: SignaturePubkeyPair;
    sigResultError?: string;
}

export function isSignatureRequestResponse(
    payload: Payload
): payload is SignatureRequestResponse {
    return (
        isBasePayload(payload) &&
        payload.type === 'signature-request-response'
    );
}
