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
