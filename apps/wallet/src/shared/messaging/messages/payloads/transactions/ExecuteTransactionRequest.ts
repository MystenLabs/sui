// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    type SuiSignTransactionInput,
    type SuiSignAndExecuteTransactionOptions,
} from '@mysten/wallet-standard';

import { isBasePayload } from '_payloads';

import type { MoveCallTransaction, SignableTransaction } from '@mysten/sui.js';
import type { BasePayload, Payload } from '_payloads';

export type TransactionDataType =
    | {
          type: 'v2';
          justSign?: boolean;
          data: SignableTransaction | string;
          options?: SuiSignAndExecuteTransactionOptions;
      }
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

export type SuiSignTransactionSerialized = Omit<
    SuiSignTransactionInput,
    'transaction'
> & {
    transaction: SignableTransaction | string;
};

export interface SignTransactionRequest extends BasePayload {
    type: 'sign-transaction-request';
    transaction: SuiSignTransactionSerialized;
}

export function isSignTransactionRequest(
    payload: Payload
): payload is SignTransactionRequest {
    return (
        isBasePayload(payload) && payload.type === 'sign-transaction-request'
    );
}
