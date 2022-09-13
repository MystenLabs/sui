// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type BN from 'bn.js';

export type DataType = {
    id: string;
    Type: string;
    _isCoin: boolean;
    Version?: string;
    display?: string;
    balance?: BN;
    name?: string;
}[];

export const ITEMS_PER_PAGE: number = 6;
