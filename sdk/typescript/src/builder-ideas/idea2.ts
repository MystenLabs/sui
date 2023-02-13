/* eslint-disable */

// These are stub implementations, you can basically ignore them:
class TransactionCommand {}
class TransactionInput {}
export class Transaction<
  Commands extends Record<string, TransactionCommand> = Record<string, never>,
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

  add<Name extends string>(
    name: Name,
    command: ({
      gas,
      reference,
      input,
    }: {
      gas: () => TransactionInput;
      reference: (name: keyof Commands) => TransactionInput;
      input: (index: number) => TransactionInput;
    }) => TransactionCommand,
  ): Transaction<Commands & { [P in Name]: TransactionCommand }> {
    return this as any;
  }

  build() {}
}

/**
 * An immutable transaction builder that uses named commands to create references between them.
 * This leverages TypeScript to type the reference interactions, and provides the gas / input through
 * a function that gets invoked when constructing the transaction.
 */

const transaction = new Transaction()
  .add('split', ({ gas, input }) => Transaction.Split(gas(), input(0)))
  .add('transfer', ({ reference, input }) =>
    // NOTE: The reference here is actually typechecked:
    Transaction.Transfer(reference('split'), input(1)),
  );

transaction.build();
