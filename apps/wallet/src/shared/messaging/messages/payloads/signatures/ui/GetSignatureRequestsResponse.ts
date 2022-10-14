// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isBasePayload } from '_payloads';

import type { BasePayload, Payload } from '_payloads';
import type { ExecuteSignatureRequest } from "_payloads/signatures";

export interface GetSignatureRequestsResponse extends BasePayload {
    type: 'get-signature-requests-response';
    sigRequests: ExecuteSignatureRequest[];
}

export function isGetSignatureRequestsResponse(
    payload: Payload
): payload is GetSignatureRequestsResponse {
    return (
        isBasePayload(payload) &&
        payload.type === 'get-signature-requests-response'
    );
}
