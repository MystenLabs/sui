// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  assert,
  literal,
  object,
  Infer,
  array,
  optional,
  define,
} from 'superstruct';
import { Provider } from '../providers/provider';
import {
  Commands,
  TransactionArgument,
  TransactionCommand,
  TransactionInput,
} from './Commands';
import { create } from './utils';

type TransactionResult = TransactionArgument & TransactionArgument[];

function createTransactionResult(index: number): TransactionResult {
  const baseResult: TransactionArgument = { kind: 'Result', index };

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

const GasConfig = object({
  gasBudget: optional(StringEncodedBigint),
  gasPrice: optional(StringEncodedBigint),
  // TODO: Do we actually need these?
  // gasPayment?: ObjectId;
  // gasOwner?: SuiAddress;
});

/**
 * The serialized representation of a transaction builder, which is used to pass
 * payloads across
 */
const SerializedTransactionBuilder = object({
  version: literal(1),
  inputs: array(TransactionInput),
  commands: array(TransactionCommand),
  gasConfig: GasConfig,
});
type SerializedTransactionBuilder = Infer<typeof SerializedTransactionBuilder>;

/**
 * Transaction Builder
 * @experimental
 */
export class Transaction<Inputs extends string = never> {
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
    tx.#inputs = parsed.inputs;
    tx.#commands = parsed.commands;
    tx.#gasConfig = parsed.gasConfig;
    return tx;
  }

  /** A helper to retrieve the Transaction builder `Commands` */
  static get Commands() {
    return Commands;
  }

  /** The gas configuration for the transaction. */
  #gasConfig: Infer<typeof GasConfig>;
  /**
   * The list of inputs currently assigned to this transaction.
   * This list should be append-only, so that indexes for arguments never change.
   */
  #inputs: TransactionInput[];
  /**
   * The list of comamnds in the transaction.
   * This list should be append-only, so that indexes for arguments never change.
   */
  #commands: TransactionCommand[];

  constructor({ inputs = [] }: { inputs?: Inputs[] } = {}) {
    this.#inputs = inputs.map((name, index) =>
      create({ kind: 'Input', index, name }, TransactionInput),
    );
    this.#commands = [];
    this.#gasConfig = {};
  }

  setGasPrice(price: number | bigint) {
    this.#gasConfig.gasPrice = String(price);
  }

  setGasBudget(budget: number | bigint) {
    this.#gasConfig.gasBudget = String(budget);
  }

  // Dynamically create a new input, which is separate from the `input`. This is important
  // for generated clients to be able to define unique inputs that are non-overlapping with the
  // defined inputs.
  createInput(value?: unknown) {
    const index = this.#inputs.length;
    const input = create({ kind: 'Input', value, index }, TransactionInput);
    this.#inputs.push(input);
    return input;
  }

  /**
   * Returns an input reference, based on the name configured in the `Transaction` constructor.
   * This input can be used as an argument in commands.
   */
  input(inputName: Inputs): TransactionInput {
    if (!inputName) {
      throw new Error('Invalid input name');
    }

    const input = this.#inputs.find((input) => inputName === input.name);

    if (!input) {
      throw new Error(`Input "${inputName}" not recognized`);
    }

    return input;
  }

  /** Returns an argument for the gas coin, to be used in a transaction. */
  gas(): TransactionArgument {
    return { kind: 'GasCoin' };
  }

  /** Add a command to the transaction. */
  add(command: TransactionCommand) {
    // TODO: This should also look at the command arguments and add any referenced commands that are not present in this transaction.
    const index = this.#commands.push(command);
    return createTransactionResult(index - 1);
  }

  /** Define the input values for the named inputs in the transaction. */
  provideInputs(inputs: Partial<Record<Inputs, unknown>>) {
    this.#inputs.forEach((input) => {
      if (!input.name) return;
      const inputValue = inputs[input.name as Inputs];
      if (inputValue) {
        input.value = inputValue;
      }
    });
  }

  /** Build the transaction to BCS bytes. */
  async build({ provider }: { provider: Provider }): Promise<Uint8Array> {
    if (!this.#gasConfig.gasPrice) {
      this.#gasConfig.gasPrice = String(await provider.getReferenceGasPrice());
    }

    throw new Error('Not implemented');
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
