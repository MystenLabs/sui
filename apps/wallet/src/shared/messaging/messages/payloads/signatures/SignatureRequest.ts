// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SuiSignMessageOutput } from '@mysten/wallet-standard';

export interface SignatureRequest {
  id: string;
  signed: boolean | null;
  origin: string;
  originFavIcon?: string;
  sigResult?: SuiSignMessageOutput;
  sigResultError?: string;
  createdDate: string;
  message: Uint8Array;
}
