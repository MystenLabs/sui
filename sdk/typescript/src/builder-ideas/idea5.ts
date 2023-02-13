/* eslint-disable */

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

  static SUIWithBalance(balance: number) {
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
 * An experiment on idea 4, which also adds an input helper function `SUIWithBalance`.
 * I imagine this would basically return a `TransactionInput` that is the result of a command
 * that is not the transaction itself, but we could detect that and automatically
 * throw it into the parent transaction for the user.
 * This is essentially how we make transaction construction for dapps as easy as possible.
 */

const transaction = new Transaction({ inputs: ['amount', 'address'] });

transaction.add(
  Transaction.Transfer(
    Transaction.SUIWithBalance(100),
    transaction.input('amount'),
  ),
);

transaction.build();
