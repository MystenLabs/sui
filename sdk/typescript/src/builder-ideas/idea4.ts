/* eslint-disable */

// These are stub implementations, you can basically ignore them:
class TransactionCommand {}
class TransactionInput {}
export class Transaction<Inputs extends string> {
  static Split(input: TransactionInput, amount: TransactionInput) {
    return new TransactionCommand();
  }

  static Transfer(coin: TransactionInput, recipient: TransactionInput) {
    return new TransactionCommand();
  }

  static MoveCall() {
    return new TransactionCommand();
  }

  static Gas() {
    return new TransactionInput();
  }

  constructor({ inputs }: { inputs: Inputs[] }) {}

  input(name: Inputs): TransactionInput {
    return new TransactionInput();
  }

  add(command: TransactionCommand): TransactionInput {
    return new TransactionInput();
  }

  build() {}
}

/**
 * Closer to idea 1, but using named inputs defined at the construction of the builder,
 * which can then be referenced using a function hanging off the builder.
 *
 * This would still allow inputs to be dynamically added (because the builder is mutable).
 * I haven't coded it out, but it should be simple.
 * Input values would still be provided at at the transaction build() call similar to
 * idea3's `provideInputs`.
 */

const transaction = new Transaction({ inputs: ['amount', 'address'] });

const splitCoin = transaction.add(
  Transaction.Split(Transaction.Gas(), transaction.input('amount')),
);

transaction.add(Transaction.Transfer(splitCoin, transaction.input('amount')));

transaction.build();
