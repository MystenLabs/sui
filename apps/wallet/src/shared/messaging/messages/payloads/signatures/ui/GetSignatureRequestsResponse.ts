// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    isBasePayload,
    type BasePayload,
    type Payload
} from '_payloads';

import type { SignatureRequest } from '../SignatureRequest';
export interface GetSignatureRequestsResponse extends BasePayload {
    type: 'get-signature-requests-response';
    sigRequests: SignatureRequest[];
}

export function isGetSignatureRequestsResponse(
    payload: Payload
): payload is GetSignatureRequestsResponse {
    return (
        isBasePayload(payload) &&
        payload.type === 'get-signature-requests-response'
    );
}
