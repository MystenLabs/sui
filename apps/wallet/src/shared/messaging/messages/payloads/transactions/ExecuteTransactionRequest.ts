// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isBasePayload } from '_payloads';

import type { MoveCallTransaction, SignableTransaction } from '@mysten/sui.js';
import type { BasePayload, Payload } from '_payloads';

export type TransactionDataType =
    | { type: 'v2'; data: SignableTransaction }
    | { type: 'move-call'; data: MoveCallTransaction }
    | { type: 'serialized-move-call'; data: string };

export interface ExecuteTransactionRequest extends BasePayload {
    type: 'execute-transaction-request';
    transaction: TransactionDataType;
}

export function isExecuteTransactionRequest(
    payload: Payload
): payload is ExecuteTransactionRequest {
    return (
        isBasePayload(payload) && payload.type === 'execute-transaction-request'
    );
}
