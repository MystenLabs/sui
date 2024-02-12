// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
export type ApiLockedObject = {
  id: string;
  objectId: string;
  keyId: string;
  owner?: string;
  itemId: string;
  deleted: boolean;
};

export type ApiEscrowObject = {
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
  deleted?: string;
  keyId?: string;
};
