// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  array,
  assert,
  define,
  Infer,
  integer,
  literal,
  nullable,
  object,
  optional,
  string,
  union,
} from 'superstruct';
import { normalizeSuiAddress, SuiObjectRef } from '../types';
import { builder } from './bcs';
import { TransactionCommand, TransactionInput } from './Commands';
import { BuilderCallArg } from './Inputs';
import { create } from './utils';

export const TransactionExpiration = optional(
  nullable(
    union([
      object({ Epoch: integer() }),
      object({ None: union([literal(true), literal(null)]) }),
    ]),
  ),
);
export type TransactionExpiration = Infer<typeof TransactionExpiration>;

const SuiAddress = string();

const StringEncodedBigint = define<string>('StringEncodedBigint', (val) => {
  if (!['string', 'number', 'bigint'].includes(typeof val)) return false;

  try {
    BigInt(val as string);
    return true;
  } catch {
    return false;
  }
});

const GasConfig = object({
  budget: optional(StringEncodedBigint),
  price: optional(StringEncodedBigint),
  payment: optional(array(SuiObjectRef)),
  owner: optional(SuiAddress),
});
type GasConfig = Infer<typeof GasConfig>;

export const SerializedTransactionDataBuilder = object({
  version: literal(1),
  sender: optional(SuiAddress),
  expiration: TransactionExpiration,
  gasConfig: GasConfig,
  inputs: array(TransactionInput),
  commands: array(TransactionCommand),
});
export type SerializedTransactionDataBuilder = Infer<
  typeof SerializedTransactionDataBuilder
>;

function prepareSuiAddress(address: string) {
  return normalizeSuiAddress(address).replace('0x', '');
}

export class TransactionDataBuilder {
  static fromBytes(bytes: Uint8Array) {
    const data = builder.de('TransactionData', bytes);
    const programmableTx = data?.V1?.kind?.Single?.ProgrammableTransaction;
    if (!programmableTx) {
      throw new Error('Unable to deserialize from bytes.');
    }

    const serialized = create(
      {
        version: 1,
        sender: data.V1.sender,
        expiration: data.V1.expiration,
        gasConfig: data.V1.gasData,
        inputs: programmableTx.inputs.map((value: unknown, index: number) =>
          create({ kind: 'Input', value, index }, TransactionInput),
        ),
        commands: programmableTx.commands,
      },
      SerializedTransactionDataBuilder,
    );

    const transactionData = new TransactionDataBuilder();
    Object.assign(transactionData, serialized);
    return transactionData;
  }

  static restore(data: SerializedTransactionDataBuilder) {
    assert(data, SerializedTransactionDataBuilder);
    const transactionData = new TransactionDataBuilder();
    Object.assign(transactionData, data);
    return transactionData;
  }

  version = 1 as const;
  sender?: string;
  expiration?: TransactionExpiration;
  gasConfig: GasConfig;
  inputs: TransactionInput[];
  commands: TransactionCommand[];

  constructor(clone?: TransactionDataBuilder) {
    this.sender = clone?.sender;
    this.expiration = clone?.expiration;
    this.gasConfig = clone?.gasConfig ?? {};
    this.inputs = clone?.inputs ?? [];
    this.commands = clone?.commands ?? [];
  }

  build({ size }: { size?: number } = {}) {
    if (!this.gasConfig.budget) {
      throw new Error('Missing gas budget');
    }

    if (!this.gasConfig.payment) {
      throw new Error('Missing gas payment');
    }

    if (!this.gasConfig.price) {
      throw new Error('Missing gas price');
    }

    if (!this.sender) {
      throw new Error('Missing transaction sender');
    }

    // Resolve inputs down to values:
    const inputs = this.inputs.map((input) => {
      assert(input.value, BuilderCallArg);
      return input.value;
    });

    const transactionData = {
      sender: prepareSuiAddress(this.sender),
      expiration: this.expiration ? this.expiration : { None: true },
      gasData: {
        payment: this.gasConfig.payment,
        owner: prepareSuiAddress(this.gasConfig.owner ?? this.sender),
        price: BigInt(this.gasConfig.price),
        budget: BigInt(this.gasConfig.budget),
      },
      kind: {
        Single: {
          ProgrammableTransaction: {
            inputs,
            commands: this.commands,
          },
        },
      },
    };

    return builder
      .ser('TransactionData', { V1: transactionData }, size)
      .toBytes();
  }

  snapshot(): SerializedTransactionDataBuilder {
    const allInputsProvided = this.inputs.every((input) => !!input.value);

    if (!allInputsProvided) {
      throw new Error('All input values must be provided before serializing.');
    }

    return create(this, SerializedTransactionDataBuilder);
  }
}
