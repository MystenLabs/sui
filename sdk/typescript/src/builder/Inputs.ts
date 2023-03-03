// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  array,
  boolean,
  Infer,
  integer,
  object,
  string,
  union,
} from 'superstruct';
import { SharedObjectRef, SuiObjectRef } from '../types';
import { builder } from './bcs';

const ObjectArg = union([
  object({ ImmOrOwned: SuiObjectRef }),
  object({
    Shared: object({
      objectId: string(),
      initialSharedVersion: integer(),
      mutable: boolean(),
    }),
  }),
]);

export const BuilderCallArg = union([
  object({ Pure: array(integer()) }),
  object({ Object: ObjectArg }),
]);
export type BuilderCallArg = Infer<typeof BuilderCallArg>;

export const Inputs = {
  Pure(type: string, data: unknown): BuilderCallArg {
    return { Pure: Array.from(builder.ser(type, data).toBytes()) };
  },
  ObjectRef(ref: SuiObjectRef): BuilderCallArg {
    return { Object: { ImmOrOwned: ref } };
  },
  SharedObjectRef(ref: SharedObjectRef): BuilderCallArg {
    return { Object: { Shared: ref } };
  },
};
