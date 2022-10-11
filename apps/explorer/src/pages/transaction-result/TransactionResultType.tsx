// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type {
    CertifiedTransaction,
    ExecutionStatusType,
    SuiObjectRef,
    SuiEvent,
} from '@mysten/sui.js';

export type DataType = CertifiedTransaction & {
    loadState: string;
    txId: string;
    status: ExecutionStatusType;
    gasFee: number;
    txError: string;
    mutated: SuiObjectRef[];
    created: SuiObjectRef[];
    events?: SuiEvent[];
    timestamp_ms: number | null;
};

export type Category =
    | 'objects'
    | 'transactions'
    | 'addresses'
    | 'ethAddress'
    | 'unknown';
