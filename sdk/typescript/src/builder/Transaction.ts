// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  assert,
  create,
  literal,
  object,
  Infer,
  array,
  any,
} from 'superstruct';
import { Provider } from '../providers/provider';
import { Commands, TransactionArgument, TransactionCommand } from './Commands';

class TransactionResult {
  // TODO: Avoid keeping track of the command index, to allow it to be computed
  #index: number;
  constructor(index: number) {
    this.#index = index;
  }

  get index() {
    return this.#index;
  }

  // TODO: Move this to array-based format instead of this:
  // TODO: Instead of making this return a concrete argument, we should ideally
  // make it reference-based (so that this gets resolved at build-time), which
  // allows re-ordering transactions.
  result(index?: number): TransactionArgument {
    if (typeof index === 'number') {
      return { kind: 'NestedResult', index: this.#index, resultIndex: index };
    }
    return { kind: 'Result', index: this.#index };
  }
}

// TODO: Does this need to be a class? Can this instead just be a `TransactionArgument` with hidden implementation details?
class TransactionInput {
  #name?: string;
  #value: unknown;
  #index: number;

  // This allows instances to be used as a `TransactionArgument`
  kind = 'Input' as const;

  // TODO: better argument order here to avoid weirdness with name.
  constructor(index: number, initialValue?: unknown, name?: string) {
    this.#index = index;
    this.#name = name;
    this.#value = initialValue;
  }

  get index() {
    return this.#index;
  }

  /** The optional debug name for the input. */
  get name() {
    return this.#name;
  }

  getValue() {
    return this.#value;
  }

  setValue(value: unknown) {
    this.#value = value;
  }
}

/**
 * The serialized representation of a transaction builder, which is used to pass
 * payloads across
 */
const SerializedTransactionBuilder = object({
  version: literal(1),
  // TODO: Need to figure out an over-the-wire input encoding.
  inputs: array(any()),
  commands: array(TransactionCommand),
});
type SerializedTransactionBuilder = Infer<typeof SerializedTransactionBuilder>;

// TODO: Support gas configuration.
export class Transaction<Inputs extends string = never> {
  static is(obj: unknown): obj is Transaction {
    return obj instanceof Transaction;
  }

  // TODO: Support fromBytes.
  static from(serialized: string) {
    const parsed = JSON.parse(serialized);
    assert(parsed, SerializedTransactionBuilder);
    const tx = new Transaction({});
    tx.#inputs = parsed.inputs.map(
      (value, i) => new TransactionInput(i, value),
    );
    tx.#commands = parsed.commands;
    return tx;
  }

  static get Commands() {
    return Commands;
  }

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

  constructor({ inputs = [] }: { inputs?: Inputs[] }) {
    this.#inputs = inputs.map(
      (name, i) => new TransactionInput(i, undefined, name),
    );
    this.#commands = [];
  }

  // Dynamically create a new input, which is separate from the `input`. This is important
  // for generated clients to be able to define unique inputs that are non-overlapping with the
  // defined inputs.
  createInput(initialValue?: unknown) {
    const index = this.#inputs.length;
    const input = new TransactionInput(index, initialValue);
    this.#inputs.push(input);
    return input;
  }

  // Get an input by the name used in the constructor:
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

  // TODO: Does this belong in the transaction? It's not actually stateful (right now),
  // and I'm not convinced that it ever will be stateful.
  gas(): TransactionArgument {
    return { kind: 'GasCoin' };
  }

  // TODO: This could also look at the command arguments and add
  // any referenced commands that are not present in this transaction.
  add(command: TransactionCommand) {
    const index = this.#commands.push(command);
    return new TransactionResult(index);
  }

  /**
   * Define the input values for the named inputs in the transaction.
   */
  provideInputs(inputs: Partial<Record<Inputs, unknown>>) {
    this.#inputs.forEach((input) => {
      if (!input.name) return;
      const inputValue = inputs[input.name as Inputs];
      if (inputValue) {
        input.setValue(inputValue);
      }
    });
  }

  async build(_: { provider: Provider }): Promise<Uint8Array> {
    throw new Error('Not implemented');
  }

  serialize() {
    // TODO: Do input values need to be provided before we serialize?
    // The wallet can fill out things like gas coin, but should we expect it to
    // also know how to fill in other types?
    // It might make sense for some things though, like other non-SUI coin types.
    // I need to ask around and see what the intuition is here, specifically:
    // - Do we expect expect the wallet to be able to fill out non-SUI coin inputs.
    //
    // If we do though, that's probably _fairly_ simple to do, using similar mechanisms
    // to the Sui coin. Basically, don't put it as a named input (because those are user-provided),
    // and have some special internal representation for places we use coins in commands.
    // Then, when we construct the transaction, we just need to walk through the commands
    // and determine all of the coin objects that we need (annoying but possible).
    // We could also keep track of this in internal state to avoid the traversal,
    // but the added benefit of the traversal is that if we update the logic, there's
    // less likelihood of state sync issues due to incompatible serializations of the
    // transaction.
    const allInputsProvided = this.#inputs.every((input) => !!input.getValue());

    if (!allInputsProvided) {
      throw new Error('All input values must be provided before serializing.');
    }

    const data: SerializedTransactionBuilder = {
      version: 1,
      inputs: this.#inputs.map((input) => input.getValue()),
      commands: this.#commands,
    };

    return JSON.stringify(create(data, SerializedTransactionBuilder));
  }
}
