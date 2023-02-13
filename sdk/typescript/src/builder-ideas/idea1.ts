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

  static Input(index: number) {
    return new TransactionInput();
  }

  add(command: TransactionCommand): TransactionInput {
    return new TransactionInput();
  }

  build() {}
}

/**
 * Traditional transaction builder. This doesn't leverage TS as much as we could,
 * but does do what we need it to do.
 */

// Create a new transaction builder
const transaction = new Transaction();

// Add a command within this transaction, and get a reference to it (which can be subsequently used).
const splitCoin = transaction.add(
  // This constructs a specific split command
  Transaction.Split(
    // Reference to the gas object
    Transaction.Gas(),
    // Reference to a specific input index (defined at transaction build time)
    Transaction.Input(0),
  ),
);

// Add another command
transaction.add(
  Transaction.Transfer(
    // Reference the result of the split command
    splitCoin,
    Transaction.Input(1),
  ),
);

// Serialize transaction or something:
transaction.build();
