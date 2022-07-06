// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isBasePayload } from '_payloads';

import type { BasePayload, Payload } from '_payloads';

export interface GetTransactionBytesRequests extends BasePayload {
    type: 'get-transaction-bytes-requests';
}

export function isGetTransactionBytesRequests(
    payload: Payload
): payload is GetTransactionBytesRequests {
    return (
        isBasePayload(payload) && payload.type === 'get-transaction-bytes-requests'
    );
}
