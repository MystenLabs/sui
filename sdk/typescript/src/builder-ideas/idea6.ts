/* eslint-disable */

// These are stub implementations, you can basically ignore them:
class TransactionCommand {}
class TransactionInput {}
export class Transaction {
  static Gas() {
    return new TransactionInput();
  }

  withSplit(coin: TransactionInput, recipient: TransactionInput) {
    return new TransactionCommand();
  }

  withTransfer(coin: TransactionInput, recipient: TransactionInput) {
    return new TransactionCommand();
  }

  withMoveCall() {
    return new TransactionCommand();
  }

  input() {
    return new TransactionInput();
  }

  build() {}
}

/**
 * This is actually a variation of idea 1.5, but it doesn't have a generic `add` interface, and instead opts for
 * explicit methods per command.
 */

// Create a new transaction builder
const transaction = new Transaction();

const amount = transaction.input();
const address = transaction.input();

const splitCoin = transaction.withSplit(Transaction.Gas(), amount);
transaction.withTransfer(splitCoin, address);

// Serialize transaction or something:
transaction.build();
