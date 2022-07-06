// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Base64DataBuffer } from '@mysten/sui.js';
import type { TransactionResponse } from '@mysten/sui.js';

export type TransactionBytesRequest = {
    id: string;
    approved: boolean | null;
    txBytes: Base64DataBuffer;
    origin: string;
    originFavIcon?: string;
    txResult?: TransactionResponse;
    txResultError?: string;
    createdDate: string;
};
