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

export const TransactionInput = object({
  kind: literal('Input'),
  index: integer(),
  name: optional(string()),
  value: optional(any()),
});
export type TransactionInput = Infer<typeof TransactionInput>;

export const CommandArgument = union([
  TransactionInput,
  object({ kind: literal('GasCoin') }),
  object({ kind: literal('Result'), index: integer() }),
  object({
    kind: literal('NestedResult'),
    index: integer(),
    resultIndex: integer(),
  }),
]);
export type CommandArgument = Infer<typeof CommandArgument>;

export const MoveCallCommand = union([
  object({
    kind: literal('MoveCall'),
    typeArguments: array(string()),
    arguments: array(CommandArgument),
    target: string(),
  }),
  object({
    kind: literal('MoveCall'),
    typeArguments: array(string()),
    arguments: array(CommandArgument),
    package: string(),
    module: string(),
    function: string(),
  }),
]);
export type MoveCallCommand = Infer<typeof MoveCallCommand>;

export const TransferObjectsCommand = object({
  kind: literal('TransferObjects'),
  objects: array(CommandArgument),
  address: CommandArgument,
});
export type TransferObjectsCommand = Infer<typeof TransferObjectsCommand>;

export const SplitCoinCommand = object({
  kind: literal('SplitCoin'),
  coin: CommandArgument,
  amount: CommandArgument,
});
export type SplitCoinCommand = Infer<typeof SplitCoinCommand>;

export const MergeCoinsCommand = object({
  kind: literal('MergeCoins'),
  coin: CommandArgument,
  coins: array(CommandArgument),
});
export type MergeCoinsCommand = Infer<typeof MergeCoinsCommand>;

export const MakeMoveVecCommand = object({
  kind: literal('MakeMoveVec'),
  type: optional(string()),
  objects: array(CommandArgument),
});
export type MakeMoveVecCommand = Infer<typeof MakeMoveVecCommand>;

export const PublishCommand = object({
  kind: literal('Publish'),
  modules: array(array(integer())),
});
export type PublishCommand = Infer<typeof PublishCommand>;

export const TransactionCommand = union([
  MoveCallCommand,
  TransferObjectsCommand,
  SplitCoinCommand,
  MergeCoinsCommand,
  PublishCommand,
  MakeMoveVecCommand,
]);
export type TransactionCommand = Infer<typeof TransactionCommand>;

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
    return { kind: 'MoveCall', ...input };
  },
  TransferObjects(
    objects: CommandArgument[],
    address: CommandArgument,
  ): TransferObjectsCommand {
    return { kind: 'TransferObjects', objects, address };
  },
  SplitCoin(coin: CommandArgument, amount: CommandArgument): SplitCoinCommand {
    return { kind: 'SplitCoin', coin, amount };
  },
  MergeCoins(
    coin: CommandArgument,
    coins: CommandArgument[],
  ): MergeCoinsCommand {
    return { kind: 'MergeCoins', coin, coins };
  },
  Publish(modules: number[][]): PublishCommand {
    return { kind: 'Publish', modules };
  },
  // TODO:
  MakeMoveVec() {},
};
