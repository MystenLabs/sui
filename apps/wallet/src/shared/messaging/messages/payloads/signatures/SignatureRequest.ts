// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export interface SignatureRequest {
  id: string;
  signed: boolean | null;
  origin: string;
  originFavIcon?: string;
  sigResult?: Uint8Array;
  sigResultError?: string;
  createdDate: string;
  message: Uint8Array;
}
