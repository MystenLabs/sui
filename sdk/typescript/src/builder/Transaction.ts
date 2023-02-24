// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { toB58 } from '@mysten/bcs';
import {
  is,
  assert,
  literal,
  object,
  Infer,
  array,
  optional,
  define,
  any,
  string,
  nullable,
  integer,
} from 'superstruct';
import { Provider } from '../providers/provider';
import { builder } from './bcs';
import {
  Commands,
  CommandArgument,
  TransactionCommand,
  TransactionInput,
} from './Commands';
import { CallArg } from './Inputs';
import { create } from './utils';

type TransactionResult = CommandArgument & CommandArgument[];

function createTransactionResult(index: number): TransactionResult {
  const baseResult: CommandArgument = { kind: 'Result', index };

  return new Proxy(baseResult, {
    set() {
      throw new Error(
        'The transaction result is a proxy, and does not support setting properties directly',
      );
    },
    // TODO: Instead of making this return a concrete argument, we should ideally
    // make it reference-based (so that this gets resolved at build-time), which
    // allows re-ordering transactions.
    get(target, property) {
      // This allows this transaction argument to be used in the singular form:
      if (property in target) {
        return Reflect.get(target, property);
      }

      // Support destructuring:
      if (property === Symbol.iterator) {
        return function* () {
          let i = 0;
          while (true) {
            yield { kind: 'NestedResult', index, resultIndex: i };
            i++;
          }
        };
      }

      if (typeof property === 'symbol') {
        throw new Error(
          `Unexpected symbol property access: "${String(property)}"`,
        );
      }

      const resultIndex = parseInt(property, 10);
      if (Number.isNaN(resultIndex) || resultIndex < 0) {
        throw new Error(`Invalid result index: "${property}"`);
      }

      // TODO: Rather than dynamically construct this, should we share a cache for the destructured / array properties
      // so that they return the same reference every time?
      return { kind: 'NestedResult', index, resultIndex };
    },
  }) as TransactionResult;
}

const StringEncodedBigint = define<string>('StringEncodedBigint', (val) => {
  if (typeof val !== 'string') return false;

  try {
    BigInt(val);
    return true;
  } catch {
    return false;
  }
});

const SuiAddress = string();
type SuiAddress = Infer<typeof SuiAddress>;

const GasConfig = object({
  budget: optional(StringEncodedBigint),
  price: optional(StringEncodedBigint),
  // TODO: Define types for gas payment:
  payment: optional(any()),
  owner: optional(SuiAddress),
});
type GasConfig = Infer<typeof GasConfig>;

const TransactionExpiration = optional(nullable(object({ Epoch: integer() })));
type TransactionExpiration = Infer<typeof TransactionExpiration>;

/**
 * The serialized representation of a transaction builder, which is used to pass
 * payloads across
 */
const SerializedTransactionBuilder = object({
  version: literal(1),
  sender: optional(SuiAddress),
  expiration: TransactionExpiration,
  inputs: array(TransactionInput),
  commands: array(TransactionCommand),
  gasConfig: GasConfig,
});
type SerializedTransactionBuilder = Infer<typeof SerializedTransactionBuilder>;

// TODO: Improve error messaging so that folks know exactly what is missing
function expectProvider(provider?: Provider): Provider {
  if (!provider) {
    throw new Error(
      'No provider passed to Transaction#build, but transaction data was not sufficient to build offline.',
    );
  }

  return provider;
}

/**
 * Transaction Builder
 * @experimental
 */
export class Transaction {
  /** Returns `true` if the object is an instance of the Transaction builder class. */
  static is(obj: unknown): obj is Transaction {
    return obj instanceof Transaction;
  }

  /**
   * Converts from a serialized transaction format to a `Transaction` class.
   * There are two supported serialized formats:
   * - A string returned from `Transaction#serialize`. The serialized format must be compatible, or it will throw an error.
   * - A byte array (or base64-encoded bytes) containing BCS transaction data.
   */
  static from(serialized: string | Uint8Array) {
    // Check for bytes:
    if (typeof serialized !== 'string' || !serialized.startsWith('{')) {
      // TODO: Support fromBytes.
      throw new Error('from() does not yet support bytes');
    }

    const parsed = JSON.parse(serialized);
    assert(parsed, SerializedTransactionBuilder);
    const tx = new Transaction();
    tx.#sender = parsed.sender;
    tx.#expiration = parsed.expiration;
    tx.#gasConfig = parsed.gasConfig;
    tx.#inputs = parsed.inputs;
    tx.#commands = parsed.commands;
    return tx;
  }

  /** A helper to retrieve the Transaction builder `Commands` */
  static get Commands() {
    return Commands;
  }

  #sender?: string;
  get sender() {
    return this.#sender;
  }
  setSender(sender: string) {
    this.#sender = sender;
  }

  #expiration?: TransactionExpiration;
  get expiration() {
    return this.#expiration;
  }
  setExpiration(expiration?: TransactionExpiration) {
    this.#expiration = expiration;
  }

  /** The gas configuration for the transaction. */
  #gasConfig: GasConfig;
  /** Returns a copy of the gas config. */
  get gasConfig(): GasConfig {
    return { ...this.#gasConfig };
  }
  setGasPrice(price: number | bigint) {
    this.#gasConfig.price = String(price);
  }
  setGasBudget(budget: number | bigint) {
    this.#gasConfig.budget = String(budget);
  }
  setGasPayment(payment: unknown) {
    this.#gasConfig.payment = payment;
  }

  /**
   * The list of inputs currently assigned to this transaction.
   * This list should be append-only, so that indexes for arguments never change.
   */
  #inputs: TransactionInput[];
  /** Returns a copy of the inputs. */
  get inputs(): TransactionInput[] {
    return [...this.#inputs];
  }

  /**
   * The list of comamnds in the transaction.
   * This list should be append-only, so that indexes for arguments never change.
   */
  #commands: TransactionCommand[];
  /** Returns a copy of the commands. */
  get commands(): TransactionCommand[] {
    return [...this.#commands];
  }

  constructor(transaction?: Transaction) {
    this.#inputs = transaction?.inputs ?? [];
    this.#commands = transaction?.commands ?? [];
    this.#gasConfig = transaction?.gasConfig ?? {};
  }

  /** Returns an argument for the gas coin, to be used in a transaction. */
  get gas(): CommandArgument {
    return { kind: 'GasCoin' };
  }

  /**
   * Dynamically create a new input, which is separate from the `input`. This is important
   * for generated clients to be able to define unique inputs that are non-overlapping with the
   * defined inputs.
   *
   * For `Uint8Array` type automatically convert the input into a `Pure` CallArg, since this
   * is the format required for custom serialization.
   *
   * For `
   */
  input(value?: unknown) {
    // For Uint8Array
    // if (value instanceof Uint8Array) {
    //   value = { Pure: value };
    // }

    const index = this.#inputs.length;
    const input = create({ kind: 'Input', value, index }, TransactionInput);
    this.#inputs.push(input);
    return input;
  }

  // TODO: Do we want to support these helper functions for inputs?
  // Maybe we can make an `Inputs` helper like commands that works seamlessly with these.
  // objectRef() {}
  // sharedObjectRef() {}
  // pure() {}

  /** Add a command to the transaction. */
  add(command: TransactionCommand) {
    // TODO: This should also look at the command arguments and add any referenced commands that are not present in this transaction.
    const index = this.#commands.push(command);
    return createTransactionResult(index - 1);
  }

  /** Build the transaction to BCS bytes. */
  async build({ provider }: { provider?: Provider } = {}): Promise<Uint8Array> {
    if (!this.#gasConfig.budget) {
      throw new Error('Missing gas budget');
    }

    if (!this.#sender) {
      throw new Error('Missing transaction sender');
    }

    if (!this.#gasConfig.price) {
      this.#gasConfig.price = String(
        await expectProvider(provider).getReferenceGasPrice(),
      );
    }

    // Resolve commands:
    const commands = this.#commands;

    // TODO: Use the commands to resolve input values:
    // commands.forEach(() => {
    // });

    // Resolve inputs:
    const inputs = this.#inputs.map((input) => {
      if (is(input.value, CallArg)) {
        return input.value;
      }

      // TODO: What Input not of a known type:
      throw new Error('Unexpected input value');
    });

    const transactionData = {
      sender: this.#sender,
      expiration: this.#expiration ? this.#expiration : { None: true },
      gasData: {
        payment: {
          objectId: (Math.random() * 100000).toFixed(0).padEnd(64, '0'),
          version: BigInt((Math.random() * 10000).toFixed(0)),
          digest: toB58(
            new Uint8Array([
              0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9,
            ]),
          ),
        },
        owner: this.#gasConfig.owner ?? this.#sender,
        price: this.#gasConfig.price,
        budget: this.#gasConfig.budget,
      },
      kind: {
        Single: {
          ProgrammableTransaction: {
            inputs,
            commands,
          },
        },
      },
    };

    return builder.ser('TransactionData', transactionData).toBytes();
  }

  /**
   * Serialize the transaction to a string so that it can be sent to a separate context.
   * This is different from `build` in that it does not serialize to BCS bytes, and instead
   * uses a separate format that is unique to the transaction builder. This allows
   * us to serialize partially-complete transactions, that can then be completed and
   * built in a separate context.
   *
   * For example, a dapp can construct a transaction, but not provide gas objects
   * or a gas budget. The transaction then can be sent to the wallet, where this
   * information is automatically filled in (e.g. by querying for coin objects
   * and performing a dry run).
   */
  serialize() {
    const allInputsProvided = this.#inputs.every((input) => !!input.value);

    if (!allInputsProvided) {
      throw new Error('All input values must be provided before serializing.');
    }

    const data: SerializedTransactionBuilder = {
      version: 1,
      inputs: this.#inputs,
      commands: this.#commands,
      gasConfig: this.#gasConfig,
    };

    return JSON.stringify(create(data, SerializedTransactionBuilder));
  }
}
