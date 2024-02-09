// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
export type LockedObject = {
  id: string;
  objectId: string;
  keyId: string;
  creator: string;
  itemId: string;
  deleted: boolean;
};

export type EscrowObject = {
  id: string;
  objectId: string;
  sender: string;
  recipient: string;
  keyId: string;
  itemId: string;
  swapped: boolean;
  cancelled: boolean;
};

export type EscrowListingQuery = {
  escrowId?: string;
  sender?: string;
  recipient?: string;
  cancelled?: string;
  swapped?: string;
};

export type LockedListingQuery = {
  creator?: string;
  deleted?: string;
  keyId?: string;
};
