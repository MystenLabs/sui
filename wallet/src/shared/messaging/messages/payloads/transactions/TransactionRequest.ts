// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type {
    MoveCallTransaction,
    SuiMoveNormalizedFunction,
    SuiTransactionResponse,
} from '@mysten/sui.js';

export type TransactionRequest = {
    id: string;
    approved: boolean | null;
    origin: string;
    originFavIcon?: string;
    txResult?: SuiTransactionResponse;
    txResultError?: string;
    metadata?: SuiMoveNormalizedFunction;
    createdDate: string;
} & (
    | {
          type: 'move-call';
          tx: MoveCallTransaction;
      }
    | {
          type: 'serialized-move-call';
          txBytes: Uint8Array;
      }
);
