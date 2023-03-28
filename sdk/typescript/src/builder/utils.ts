// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { create as superstructCreate, Struct } from 'superstruct';

export function create<T, S>(value: T, struct: Struct<T, S>): T {
  return superstructCreate(value, struct);
}

export type WellKnownEncoding =
  | {
      kind: 'object';
    }
  | {
      kind: 'pure';
      type: string;
    };

export const TRANSACTION_TYPE = Symbol('transaction-argument-type');
