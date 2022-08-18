// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SignaturePubkeyPair } from '@mysten/sui.js';

export type SignMessageRequest = {
    id: string;
    approved: boolean | null;
    origin: string;
    originFavIcon?: string;
    messageData?: string; // base 64 encoded string
    messageString?: string;
    createdDate: string;
    signature?: SignaturePubkeyPair;
};
