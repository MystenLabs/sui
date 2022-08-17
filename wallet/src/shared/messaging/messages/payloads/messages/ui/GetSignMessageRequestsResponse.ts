// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isBasePayload } from '_payloads';

import type { SignMessageRequest } from '../SignMessageRequest';
import type { BasePayload, Payload } from '_payloads';

export interface GetSignMessageRequestsResponse extends BasePayload {
    type: 'get-sign-message-requests-response';
    signMessageRequests: SignMessageRequest[];
}

export function isGetSignMessageRequestsResponse(
    payload: Payload
): payload is GetSignMessageRequestsResponse {
    return (
        isBasePayload(payload) &&
        payload.type === 'get-sign-message-requests-response'
    );
}
