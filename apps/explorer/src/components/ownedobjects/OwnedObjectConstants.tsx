// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export type DataType = {
    id: string;
    Type: string;
    _isCoin: boolean;
    Version?: string;
    display?: string;
    balance?: bigint;
    name?: string;
}[];

export const ITEMS_PER_PAGE: number = 6;
