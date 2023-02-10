// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    type SuiSignAndExecuteTransactionOptions,
    type SuiSignTransactionInput,
} from '@mysten/wallet-standard';

import { isBasePayload } from '_payloads';

import type { MoveCallTransaction, SignableTransaction } from '@mysten/sui.js';
import type { BasePayload, Payload } from '_payloads';

export type TransactionDataType =
    | {
          type: 'v2';
          data: SignableTransaction;
          justSign?: boolean;
          options?: SuiSignAndExecuteTransactionOptions;
      }
    | { type: 'move-call'; data: MoveCallTransaction }
    | { type: 'serialized-move-call'; data: string };

export interface ExecuteTransactionRequest extends BasePayload {
    type: 'execute-transaction-request';
    transaction: TransactionDataType;
}

export interface SignTransactionRequest extends BasePayload {
    type: 'sign-transaction-request';
    transaction: SuiSignTransactionInput;
}

export function isSignTransactionRequest(
    payload: Payload
): payload is SignTransactionRequest {
    return (
        isBasePayload(payload) && payload.type === 'sign-transaction-request'
    );
}

export function isExecuteTransactionRequest(
    payload: Payload
): payload is ExecuteTransactionRequest {
    return (
        isBasePayload(payload) && payload.type === 'execute-transaction-request'
    );
}
