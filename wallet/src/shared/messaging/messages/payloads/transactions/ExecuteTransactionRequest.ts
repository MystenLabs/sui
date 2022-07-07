// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isBasePayload } from '_payloads';

import type { MoveCallTransaction } from '@mysten/sui.js';
import type { BasePayload, Payload } from '_payloads';

export interface ExecuteTransactionRequest extends BasePayload {
    type: 'execute-transaction-request';
    transaction?: MoveCallTransaction;
    transactionBytes?: Uint8Array;
}

export function isExecuteTransactionRequest(
    payload: Payload
): payload is ExecuteTransactionRequest {
    return (
        isBasePayload(payload) && payload.type === 'execute-transaction-request'
    );
}
