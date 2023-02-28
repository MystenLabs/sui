// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  any,
  array,
  Infer,
  integer,
  literal,
  object,
  optional,
  string,
  union,
} from 'superstruct';
import { WithTupleTag } from './utils';

export const TransactionInput = object({
  kind: literal('Input'),
  index: integer(),
  name: optional(string()),
  value: optional(any()),
});
export type TransactionInput = Infer<typeof TransactionInput>;

export const TransactionArgument = union([
  TransactionInput,
  object({ kind: literal('GasCoin') }),
  object({ kind: literal('Result'), index: integer() }),
  object({
    kind: literal('NestedResult'),
    index: integer(),
    resultIndex: integer(),
  }),
]);
export type TransactionArgument = Infer<typeof TransactionArgument>;

export const MoveCallCommand = object({
  kind: literal('MoveCall'),
  // TODO: Accept object ref or object ID:
  package: string(),
  module: string(),
  function: string(),
  typeArguments: array(string()),
  arguments: array(TransactionArgument),
});
export type MoveCallCommand = Infer<typeof MoveCallCommand>;

export const TransferObjectsCommand = object({
  kind: literal('TransferObjects'),
  objects: array(TransactionArgument),
  address: TransactionArgument,
});
export type TransferObjectsCommand = WithTupleTag<
  Infer<typeof TransferObjectsCommand>,
  0
>;

export const SplitCommand = object({
  kind: literal('Split'),
  coin: TransactionArgument,
  amount: TransactionArgument,
});
export type SplitCommand = WithTupleTag<Infer<typeof SplitCommand>, 1>;

export const MergeCommand = object({
  kind: literal('Merge'),
  coin: TransactionArgument,
  coins: array(TransactionArgument),
});
export type MergeCommand = WithTupleTag<Infer<typeof MergeCommand>, 0>;

export const TransactionCommand = union([
  MoveCallCommand,
  TransferObjectsCommand,
  SplitCommand,
  MergeCommand,
]);

export type TransactionCommand = Infer<typeof TransactionCommand>;

/**
 * Simple helpers used to construct commands:
 */
export const Commands = {
  MoveCall(input: Omit<MoveCallCommand, 'kind'>): MoveCallCommand {
    return { kind: 'MoveCall', ...input };
  },
  TransferObjects(
    objects: TransactionArgument[],
    address: TransactionArgument,
  ): TransferObjectsCommand {
    return { kind: 'TransferObjects', objects, address };
  },
  Split(coin: TransactionArgument, amount: TransactionArgument): SplitCommand {
    return { kind: 'Split', coin, amount };
  },
  Merge(coin: TransactionArgument, coins: TransactionArgument[]): MergeCommand {
    return { kind: 'Merge', coin, coins };
  },
};
