// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isBasePayload } from '_payloads';

import type { BasePayload, Payload } from '_payloads';
import type { TransactionBytesRequest } from '_payloads/transactions';

export interface GetTransactionBytesRequestsResponse extends BasePayload {
    type: 'get-transaction-bytes-requests-response';
    txBytesRequests: TransactionBytesRequest[];
}

export function isGetTransactionBytesRequestsResponse(
    payload: Payload
): payload is GetTransactionBytesRequestsResponse {
    return (
        isBasePayload(payload) &&
        payload.type === 'get-transaction-bytes-requests-response'
    );
}
