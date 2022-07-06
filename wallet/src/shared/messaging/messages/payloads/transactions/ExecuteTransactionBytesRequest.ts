// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isBasePayload } from '_payloads';
import { Base64DataBuffer } from '@mysten/sui.js';
import type { BasePayload, Payload } from '_payloads';

export interface ExecuteTransactionBytesRequest extends BasePayload {
    type: 'execute-transaction-bytes-request';
    transaction_bytes: Base64DataBuffer;
}

export function isExecuteTransactionBytesRequest(
    payload: Payload
): payload is ExecuteTransactionBytesRequest {
    return (
        isBasePayload(payload) && payload.type === 'execute-transaction-bytes-request'
    );
}
