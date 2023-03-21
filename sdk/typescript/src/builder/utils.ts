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

export const COMMAND_TYPE = Symbol('command-argument-type');

export type DeepReadonly<T> = {
  readonly [P in keyof T]: DeepReadonly<T[P]>;
};
