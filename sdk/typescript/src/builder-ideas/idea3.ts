/* eslint-disable */

// These are stub implementations, you can basically ignore them:
class TransactionCommand {}
class TransactionInput {}
export class Transaction<
  Commands extends Record<string, TransactionCommand> = Record<string, never>,
  Inputs extends Record<string, true> = {},
> {
  static Split(input: TransactionInput, amount: TransactionInput) {
    return new TransactionCommand();
  }

  static Transfer(coin: TransactionInput, recipient: TransactionInput) {
    return new TransactionCommand();
  }

  static MoveCall() {
    return new TransactionCommand();
  }

  add<Name extends string, NewInputs extends string>(register: {
    name?: Name;
    inputs?: NewInputs[];
    command: (register: {
      gas: () => TransactionInput;
      reference: (name: keyof Commands) => TransactionInput;
      input: (name: NewInputs) => TransactionInput;
    }) => TransactionCommand;
  }): Transaction<
    Commands & { [P in Name]: TransactionCommand },
    Inputs & { [P in NewInputs]: true }
  > {
    return this as any;
  }

  provideInputs(inputs: Record<keyof Inputs, any>): Inputs {
    return {} as any;
  }
}

/**
 * An immutable transaction builder, evolved from idea2. It similarly uses named commands,
 * with objects to define a given transaction. Each command also now defines its own inputs,
 * which then get type-checked when referenced within the command function.
 * When building the transaction, these inputs can be provided by name (would generate indexes).
 * We can also type-check command and input name uniqueness (although kind of annoying to have to rename).
 */

const transaction = new Transaction()
  .add({
    name: 'split',
    inputs: ['amount'],
    command: ({ gas, input }) => Transaction.Split(gas(), input('amount')),
  })
  .add({
    inputs: ['address'],
    command: ({ reference, input }) =>
      Transaction.Transfer(reference('split'), input('address')),
  });

// These are type-checked from previous functions:
transaction.provideInputs({ address: '0x2', amount: 100 });
