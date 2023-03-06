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
} from 'superstruct';
import { COMMAND_TYPE, WellKnownEncoding } from './utils';

export const TransactionInput = object({
  kind: literal('Input'),
  index: integer(),
  name: optional(string()),
  value: optional(any()),
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
  target: string(),
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
  type: optional(string()),
  objects: array(ObjectCommandArgument),
});
export type MakeMoveVecCommand = Infer<typeof MakeMoveVecCommand>;

export const PublishCommand = object({
  kind: literal('Publish'),
  modules: array(array(integer())),
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

// Refined types for move call which support both the target interface, and the
// deconstructed interface:
type MoveCallInput = (
  | {
      target: string;
      package?: never;
      module?: never;
      function?: never;
    }
  | {
      target?: never;
      package: string;
      module: string;
      function: string;
    }
) & {
  typeArguments: string[];
  arguments: CommandArgument[];
};

/**
 * Simple helpers used to construct commands:
 */
export const Commands = {
  MoveCall(input: MoveCallInput): MoveCallCommand {
    return {
      kind: 'MoveCall',
      target:
        input.target ??
        [input.package, input.module, input.function].join('::'),
      arguments: input.arguments,
      typeArguments: input.typeArguments,
    };
  },
  TransferObjects(
    // TODO: Do validation of objects being an Array.
    objects: CommandArgument[],
    address: CommandArgument,
  ): TransferObjectsCommand {
    return { kind: 'TransferObjects', objects, address };
  },
  SplitCoin(coin: CommandArgument, amount: CommandArgument): SplitCoinCommand {
    return { kind: 'SplitCoin', coin, amount };
  },
  MergeCoins(
    destination: CommandArgument,
    sources: CommandArgument[],
  ): MergeCoinsCommand {
    return { kind: 'MergeCoins', destination, sources };
  },
  Publish(modules: number[][]): PublishCommand {
    return { kind: 'Publish', modules };
  },
  // TODO:
  MakeMoveVec() {},
};
