// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
export type PoolObject = {
  objectId: string;
  digest: string;
  version: string;
  type: string;
  balance?: number;
};

/**
 * A map of pool objects, where the keys are object IDs and the values are pool objects.
 */
export type PoolObjectsMap = Map<string, PoolObject>; // Map<objectId, object>

/**
 * An alias type for a string that's representing a signature.
 */
export type Signature = string;
