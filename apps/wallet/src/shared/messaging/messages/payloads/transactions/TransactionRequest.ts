// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type {
    SuiMoveNormalizedFunction,
    SuiTransactionResponse,
    UnserializedSignableTransaction,
} from '@mysten/sui.js';
import type { TransactionDataType } from '_messages/payloads/transactions/ExecuteTransactionRequest';

export type TransactionRequest = {
    id: string;
    approved: boolean | null;
    origin: string;
    originFavIcon?: string;
    txResult?: SuiTransactionResponse;
    txResultError?: string;
    metadata?: SuiMoveNormalizedFunction;
    createdDate: string;
    tx: TransactionDataType;
    unSerializedTxn?: UnserializedSignableTransaction | null;
};
