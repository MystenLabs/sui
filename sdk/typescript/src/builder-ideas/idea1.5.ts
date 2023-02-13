/* eslint-disable */

// These are stub implementations, you can basically ignore them:
class TransactionCommand {}
class TransactionInput {}
export class Transaction {
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

  add(command: TransactionCommand): TransactionInput {
    return new TransactionInput();
  }

  input() {
    return new TransactionInput();
  }

  build() {}
}

/**
 * A slight variant of idea 1, but uses functions to get inputs,
 * which would internally use a counter instead of needing to keep track of indexes.
 */

// Create a new transaction builder
const transaction = new Transaction();

const amount = transaction.input();
const address = transaction.input();

const splitCoin = transaction.add(Transaction.Split(Transaction.Gas(), amount));
transaction.add(Transaction.Transfer(splitCoin, address));

// Serialize transaction or something:
transaction.build();
