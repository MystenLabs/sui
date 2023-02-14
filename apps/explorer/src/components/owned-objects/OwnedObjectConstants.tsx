// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// todo: remove these
export type Data = {
    id: string;
    Type: string;
    _isCoin: boolean;
    Version?: string;
    display?: string;
    balance?: bigint;
    name?: string;
};
export type DataType = Data[];

export const ITEMS_PER_PAGE: number = 6;
