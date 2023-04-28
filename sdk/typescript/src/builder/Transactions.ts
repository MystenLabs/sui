// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { BCS, fromB64 } from '@mysten/bcs';
import {
  is,
  any,
  array,
  Infer,
  integer,
  literal,
  object,
  optional,
  string,
  union,
  assert,
  Struct,
  define,
} from 'superstruct';
import { ObjectId, normalizeSuiObjectId } from '../types/common';
import { TRANSACTION_TYPE, WellKnownEncoding, create } from './utils';

const option = <T extends Struct<any, any>>(some: T) =>
  union([
    object({ None: union([literal(true), literal(null)]) }),
    object({ Some: some }),
  ]);

export const TransactionBlockInput = object({
  kind: literal('Input'),
  index: integer(),
  value: optional(any()),
  type: optional(union([literal('pure'), literal('object')])),
});
export type TransactionBlockInput = Infer<typeof TransactionBlockInput>;

const TransactionArgumentTypes = [
  TransactionBlockInput,
  object({ kind: literal('GasCoin') }),
  object({ kind: literal('Result'), index: integer() }),
  object({
    kind: literal('NestedResult'),
    index: integer(),
    resultIndex: integer(),
  }),
] as const;

// Generic transaction argument
export const TransactionArgument = union([...TransactionArgumentTypes]);
export type TransactionArgument = Infer<typeof TransactionArgument>;

// Transaction argument referring to an object:
export const ObjectTransactionArgument = union([...TransactionArgumentTypes]);
(ObjectTransactionArgument as any)[TRANSACTION_TYPE] = {
  kind: 'object',
} as WellKnownEncoding;

export const PureTransactionArgument = (type: string) => {
  const struct = union([...TransactionArgumentTypes]);
  (struct as any)[TRANSACTION_TYPE] = {
    kind: 'pure',
    type,
  } as WellKnownEncoding;
  return struct;
};

export const MoveCallTransaction = object({
  kind: literal('MoveCall'),
  target: define<`${string}::${string}::${string}`>(
    'target',
    string().validator,
  ),
  typeArguments: array(string()),
  arguments: array(TransactionArgument),
});
export type MoveCallTransaction = Infer<typeof MoveCallTransaction>;

export const TransferObjectsTransaction = object({
  kind: literal('TransferObjects'),
  objects: array(ObjectTransactionArgument),
  address: PureTransactionArgument(BCS.ADDRESS),
});
export type TransferObjectsTransaction = Infer<
  typeof TransferObjectsTransaction
>;

export const SplitCoinsTransaction = object({
  kind: literal('SplitCoins'),
  coin: ObjectTransactionArgument,
  amounts: array(PureTransactionArgument('u64')),
});
export type SplitCoinsTransaction = Infer<typeof SplitCoinsTransaction>;

export const MergeCoinsTransaction = object({
  kind: literal('MergeCoins'),
  destination: ObjectTransactionArgument,
  sources: array(ObjectTransactionArgument),
});
export type MergeCoinsTransaction = Infer<typeof MergeCoinsTransaction>;

export const MakeMoveVecTransaction = object({
  kind: literal('MakeMoveVec'),
  type: optional(option(string())),
  objects: array(ObjectTransactionArgument),
});
export type MakeMoveVecTransaction = Infer<typeof MakeMoveVecTransaction>;

export const PublishTransaction = object({
  kind: literal('Publish'),
  modules: array(array(integer())),
  dependencies: array(ObjectId),
});
export type PublishTransaction = Infer<typeof PublishTransaction>;

// Keep in sync with constants in
// crates/sui-framework/packages/sui-framework/sources/package.move
export enum UpgradePolicy {
  COMPATIBLE = 0,
  ADDITIVE = 128,
  DEP_ONLY = 192,
}

export const UpgradeTransaction = object({
  kind: literal('Upgrade'),
  modules: array(array(integer())),
  dependencies: array(ObjectId),
  packageId: ObjectId,
  ticket: ObjectTransactionArgument,
});
export type UpgradeTransaction = Infer<typeof UpgradeTransaction>;

const TransactionTypes = [
  MoveCallTransaction,
  TransferObjectsTransaction,
  SplitCoinsTransaction,
  MergeCoinsTransaction,
  PublishTransaction,
  UpgradeTransaction,
  MakeMoveVecTransaction,
] as const;

export const TransactionType = union([...TransactionTypes]);
export type TransactionType = Infer<typeof TransactionType>;

export function getTransactionType(data: unknown) {
  assert(data, TransactionType);
  return TransactionTypes.find((schema) => is(data, schema as Struct))!;
}

/**
 * Simple helpers used to construct transactions:
 */
export const Transactions = {
  MoveCall(
    input: Omit<MoveCallTransaction, 'kind' | 'arguments' | 'typeArguments'> & {
      arguments?: TransactionArgument[];
      typeArguments?: string[];
    },
  ): MoveCallTransaction {
    return create(
      {
        kind: 'MoveCall',
        target: input.target,
        arguments: input.arguments ?? [],
        typeArguments: input.typeArguments ?? [],
      },
      MoveCallTransaction,
    );
  },
  TransferObjects(
    objects: TransactionArgument[],
    address: TransactionArgument,
  ): TransferObjectsTransaction {
    return create(
      { kind: 'TransferObjects', objects, address },
      TransferObjectsTransaction,
    );
  },
  SplitCoins(
    coin: TransactionArgument,
    amounts: TransactionArgument[],
  ): SplitCoinsTransaction {
    return create({ kind: 'SplitCoins', coin, amounts }, SplitCoinsTransaction);
  },
  MergeCoins(
    destination: TransactionArgument,
    sources: TransactionArgument[],
  ): MergeCoinsTransaction {
    return create(
      { kind: 'MergeCoins', destination, sources },
      MergeCoinsTransaction,
    );
  },
  Publish({
    modules,
    dependencies,
  }: {
    modules: number[][] | string[];
    dependencies: ObjectId[];
  }): PublishTransaction {
    return create(
      {
        kind: 'Publish',
        modules: modules.map((module) =>
          typeof module === 'string' ? Array.from(fromB64(module)) : module,
        ),
        dependencies: dependencies.map((dep) => normalizeSuiObjectId(dep)),
      },
      PublishTransaction,
    );
  },
  Upgrade({
    modules,
    dependencies,
    packageId,
    ticket,
  }: {
    modules: number[][] | string[];
    dependencies: ObjectId[];
    packageId: ObjectId;
    ticket: TransactionArgument;
  }): UpgradeTransaction {
    return create(
      {
        kind: 'Upgrade',
        modules: modules.map((module) =>
          typeof module === 'string' ? Array.from(fromB64(module)) : module,
        ),
        dependencies: dependencies.map((dep) => normalizeSuiObjectId(dep)),
        packageId,
        ticket,
      },
      UpgradeTransaction,
    );
  },
  MakeMoveVec({
    type,
    objects,
  }: Omit<MakeMoveVecTransaction, 'kind' | 'type'> & {
    type?: string;
  }): MakeMoveVecTransaction {
    return create(
      {
        kind: 'MakeMoveVec',
        type: type ? { Some: type } : { None: null },
        objects,
      },
      MakeMoveVecTransaction,
    );
  },
};
