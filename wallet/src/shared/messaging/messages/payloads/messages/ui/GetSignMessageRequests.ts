// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isBasePayload } from '_payloads';

import type { BasePayload, Payload } from '_payloads';

export interface GetSignMessageRequests extends BasePayload {
    type: 'get-sign-message-requests';
}

export function isGetSignMessageRequests(
    payload: Payload
): payload is GetSignMessageRequests {
    return (
        isBasePayload(payload) && payload.type === 'get-sign-message-requests'
    );
}
