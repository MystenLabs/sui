// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { MoveCallTransaction, TransactionResponse } from '@mysten/sui.js';

export type TransactionRequest = {
    id: string;
    approved: boolean | null;
    tx: MoveCallTransaction;
    origin: string;
    originFavIcon?: string;
    txResult?: TransactionResponse;
    txResultError?: string;
    createdDate: string;
};
