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
import { TransactionArgument, TransactionCommand } from './Commands';

class TransactionInput {
  #name?: string;
  #value: unknown;
  constructor(name?: string, initialValue?: unknown) {
    this.#name = name;
    this.#value = initialValue;
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

export class Transaction<Inputs extends string> {
  static from(serialized: string) {
    const parsed = JSON.parse(serialized);
    assert(parsed, SerializedTransactionBuilder);
    const tx = new Transaction({});
    tx.#inputs = parsed.inputs.map(
      (value) => new TransactionInput(undefined, value),
    );
    tx.#commands = parsed.commands;
    return tx;
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
    this.#inputs = inputs.map((name) => new TransactionInput(name));
    this.#commands = [];
  }

  // Dynamically create a new input, which is separate from the `input`. This is important
  // for generated clients to be able to define unique inputs that are non-overlapping with the
  // defined inputs.
  createInput() {
    const input = new TransactionInput();
    this.#inputs.push(input);
    return input;
  }

  // Get an input by the name used in the constructor:
  input(inputName: Inputs): TransactionArgument {
    if (!inputName) {
      throw new Error('Invalid input name');
    }

    const inputIndex = this.#inputs.findIndex(
      (input) => inputName === input.name,
    );

    if (inputIndex === -1) {
      throw new Error(`Input "${inputName}" not recognized`);
    }

    return {
      kind: 'Input',
      index: inputIndex,
    };
  }

  // TODO: Does this belong in the transaction? It's not actually stateful (right now),
  // and I'm not convinced that it ever will be stateful.
  gas(): TransactionArgument {
    return { kind: 'GasCoin' };
  }

  // TODO: This could also look at the command arguments and add
  // any referenced commands that are not present in this transaction.
  // This sill require
  add(command: TransactionCommand) {
    this.#commands.push(command);
    // TODO: Remove as any and make `TransactionResult` struct:
    return command as any;
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

  async build() {
    throw new Error('Not implemented');
  }

  // TODO: Do input values need to be provided before we serialize? I imagine yes
  // because otherwise it's very unclear how input values would be provided.
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
  serialize() {
    const data: SerializedTransactionBuilder = {
      version: 1,
      inputs: this.#inputs.map((input) => input.getValue()),
      commands: this.#commands,
    };

    return JSON.stringify(create(data, SerializedTransactionBuilder));
  }
}
