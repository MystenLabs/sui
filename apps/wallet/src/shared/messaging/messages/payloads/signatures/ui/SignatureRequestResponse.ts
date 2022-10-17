// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    isBasePayload,
    type BasePayload,
    type Payload
} from '_payloads';

import type { SuiSignMessageOutput } from '@mysten/wallet-standard';

export interface SignatureRequestResponse extends BasePayload {
    type: 'signature-request-response';
    sigId: string;
    signed: boolean;
    sigResult?: SuiSignMessageOutput;
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
