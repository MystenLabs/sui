// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    type SuiSignTransactionInput,
    type SuiSignAndExecuteTransactionOptions,
} from '@mysten/wallet-standard';

import { isBasePayload } from '_payloads';

import type {
    MoveCallTransaction,
    SignableTransaction,
    SuiAddress,
} from '@mysten/sui.js';
import type { BasePayload, Payload } from '_payloads';

export type TransactionDataType =
    | {
          type: 'v2';
          justSign?: boolean;
          //   TODO: Support transaciton builder string
          //   data: SignableTransaction | string;
          data: SignableTransaction;
          options?: SuiSignAndExecuteTransactionOptions;
          account: SuiAddress;
      }
    | { type: 'move-call'; data: MoveCallTransaction; account: SuiAddress }
    | { type: 'serialized-move-call'; data: string; account: SuiAddress };

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
    'transaction' | 'account'
> & {
    // TODO: Enable string for serialized `Transaction`
    // transaction: SignableTransaction | string;
    transaction: SignableTransaction;
    account: SuiAddress;
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
