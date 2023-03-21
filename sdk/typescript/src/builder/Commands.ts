// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { BCS } from '@mysten/bcs';
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
import { ObjectId } from '../types/common';
import { COMMAND_TYPE, WellKnownEncoding, create } from './utils';

const option = <T extends Struct<any, any>>(some: T) =>
  union([object({ None: literal(null) }), object({ Some: some })]);

export const TransactionInput = object({
  kind: literal('Input'),
  index: integer(),
  name: optional(string()),
  value: optional(any()),
  type: optional(union([literal('pure'), literal('object')])),
});
export type TransactionInput = Infer<typeof TransactionInput>;

const CommandArgumentTypes = [
  TransactionInput,
  object({ kind: literal('GasCoin') }),
  object({ kind: literal('Result'), index: integer() }),
  object({
    kind: literal('NestedResult'),
    index: integer(),
    resultIndex: integer(),
  }),
] as const;

// Generic command argument
export const CommandArgument = union([...CommandArgumentTypes]);
export type CommandArgument = Infer<typeof CommandArgument>;

// Command argument referring to an object:
export const ObjectCommandArgument = union([...CommandArgumentTypes]);
(ObjectCommandArgument as any)[COMMAND_TYPE] = {
  kind: 'object',
} as WellKnownEncoding;

export const PureCommandArgument = (type: string) => {
  const struct = union([...CommandArgumentTypes]);
  (struct as any)[COMMAND_TYPE] = {
    kind: 'pure',
    type,
  } as WellKnownEncoding;
  return struct;
};

export const MoveCallCommand = object({
  kind: literal('MoveCall'),
  target: define<`${string}::${string}::${string}`>(
    'target',
    string().validator,
  ),
  typeArguments: array(string()),
  arguments: array(CommandArgument),
});
export type MoveCallCommand = Infer<typeof MoveCallCommand>;

export const TransferObjectsCommand = object({
  kind: literal('TransferObjects'),
  objects: array(ObjectCommandArgument),
  address: PureCommandArgument(BCS.ADDRESS),
});
export type TransferObjectsCommand = Infer<typeof TransferObjectsCommand>;

export const SplitCoinCommand = object({
  kind: literal('SplitCoin'),
  coin: ObjectCommandArgument,
  amount: PureCommandArgument('u64'),
});
export type SplitCoinCommand = Infer<typeof SplitCoinCommand>;

export const MergeCoinsCommand = object({
  kind: literal('MergeCoins'),
  destination: ObjectCommandArgument,
  sources: array(ObjectCommandArgument),
});
export type MergeCoinsCommand = Infer<typeof MergeCoinsCommand>;

export const MakeMoveVecCommand = object({
  kind: literal('MakeMoveVec'),
  type: optional(option(string())),
  objects: array(ObjectCommandArgument),
});
export type MakeMoveVecCommand = Infer<typeof MakeMoveVecCommand>;

export const PublishCommand = object({
  kind: literal('Publish'),
  modules: array(array(integer())),
  dependencies: array(ObjectId),
});
export type PublishCommand = Infer<typeof PublishCommand>;

const TransactionCommandTypes = [
  MoveCallCommand,
  TransferObjectsCommand,
  SplitCoinCommand,
  MergeCoinsCommand,
  PublishCommand,
  MakeMoveVecCommand,
] as const;

export const TransactionCommand = union([...TransactionCommandTypes]);
export type TransactionCommand = Infer<typeof TransactionCommand>;

export function getTransactionCommandType(data: unknown) {
  assert(data, TransactionCommand);
  return TransactionCommandTypes.find((schema) => is(data, schema as Struct))!;
}

/**
 * Simple helpers used to construct commands:
 */
export const Commands = {
  MoveCall(
    input: Omit<MoveCallCommand, 'kind' | 'arguments' | 'typeArguments'> & {
      arguments?: CommandArgument[];
      typeArguments?: string[];
    },
  ): MoveCallCommand {
    return create(
      {
        kind: 'MoveCall',
        target: input.target,
        arguments: input.arguments ?? [],
        typeArguments: input.typeArguments ?? [],
      },
      MoveCallCommand,
    );
  },
  TransferObjects(
    objects: CommandArgument[],
    address: CommandArgument,
  ): TransferObjectsCommand {
    return create(
      { kind: 'TransferObjects', objects, address },
      TransferObjectsCommand,
    );
  },
  SplitCoin(coin: CommandArgument, amount: CommandArgument): SplitCoinCommand {
    return create({ kind: 'SplitCoin', coin, amount }, SplitCoinCommand);
  },
  MergeCoins(
    destination: CommandArgument,
    sources: CommandArgument[],
  ): MergeCoinsCommand {
    return create(
      { kind: 'MergeCoins', destination, sources },
      MergeCoinsCommand,
    );
  },
  Publish(modules: number[][], dependencies: ObjectId[]): PublishCommand {
    return create({ kind: 'Publish', modules, dependencies }, PublishCommand);
  },
  MakeMoveVec({
    type,
    objects,
  }: Omit<MakeMoveVecCommand, 'kind' | 'type'> & {
    type?: string;
  }): MakeMoveVecCommand {
    return create(
      {
        kind: 'MakeMoveVec',
        type: type ? { Some: type } : { None: null },
        objects,
      },
      MakeMoveVecCommand,
    );
  },
};
