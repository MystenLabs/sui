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
import {
  normalizeSuiAddress,
  ObjectId,
  SharedObjectRef,
  SuiObjectRef,
} from '../types';
import { builder } from './bcs';

const ObjectArg = union([
  object({ ImmOrOwned: SuiObjectRef }),
  object({
    Shared: object({
      objectId: string(),
      initialSharedVersion: union([integer(), string()]),
      mutable: boolean(),
    }),
  }),
]);

export const PureCallArg = object({ Pure: array(integer()) });
export const ObjectCallArg = object({ Object: ObjectArg });
export type PureCallArg = Infer<typeof PureCallArg>;
export type ObjectCallArg = Infer<typeof ObjectCallArg>;

export const BuilderCallArg = union([PureCallArg, ObjectCallArg]);
export type BuilderCallArg = Infer<typeof BuilderCallArg>;

export const Inputs = {
  Pure(data: unknown, type?: string): PureCallArg {
    return {
      Pure: Array.from(
        data instanceof Uint8Array
          ? data
          : // NOTE: We explicitly set this to be growable to infinity, because we have maxSize validation at the builder-level:
            builder.ser(type!, data, { maxSize: Infinity }).toBytes(),
      ),
    };
  },
  ObjectRef({ objectId, digest, version }: SuiObjectRef): ObjectCallArg {
    return {
      Object: {
        ImmOrOwned: {
          digest,
          version,
          objectId: normalizeSuiAddress(objectId),
        },
      },
    };
  },
  SharedObjectRef({
    objectId,
    mutable,
    initialSharedVersion,
  }: SharedObjectRef): ObjectCallArg {
    return {
      Object: {
        Shared: {
          mutable,
          initialSharedVersion,
          objectId: normalizeSuiAddress(objectId),
        },
      },
    };
  },
};

export function getIdFromCallArg(arg: ObjectId | ObjectCallArg) {
  if (typeof arg === 'string') {
    return normalizeSuiAddress(arg);
  }
  if ('ImmOrOwned' in arg.Object) {
    return normalizeSuiAddress(arg.Object.ImmOrOwned.objectId);
  }
  return normalizeSuiAddress(arg.Object.Shared.objectId);
}

export function getSharedObjectInput(
  arg: BuilderCallArg,
): SharedObjectRef | undefined {
  return typeof arg === 'object' && 'Object' in arg && 'Shared' in arg.Object
    ? arg.Object.Shared
    : undefined;
}

export function isSharedObjectInput(arg: BuilderCallArg): boolean {
  return !!getSharedObjectInput(arg);
}

export function isMutableSharedObjectInput(arg: BuilderCallArg): boolean {
  return getSharedObjectInput(arg)?.mutable ?? false;
}
