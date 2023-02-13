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

  static Builder<Inputs extends (string | number)[]>(
    inputs: Inputs,
    builder: (register: {
      transaction: Transaction;
      gas: TransactionInput;
      inputs: { [P in keyof Inputs]: TransactionInput };
    }) => void,
  ) {}

  constructor() {}

  split(a: TransactionInput, b: TransactionInput) {
    return new TransactionInput();
  }
  transfer(a: TransactionInput, b: TransactionInput) {
    return new TransactionInput();
  }
}

/**
 * This is what Ashok outlined during the programmable transaction meeting,
 * but without usage of `this`.
 */

// Create a new transaction builder
const transaction = Transaction.Builder(
  ['0x2', 100],
  ({ transaction, gas, inputs: [address, amount] }) => {
    const splitCoin = transaction.split(gas, amount);
    transaction.transfer(splitCoin, address);
  },
);
