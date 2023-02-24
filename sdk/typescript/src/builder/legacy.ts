import { fromB64 } from '@mysten/bcs';
import { UnserializedSignableTransaction } from '../signers/txn-data-serializers/txn-data-serializer';
import { Transaction, Commands } from './';

/**
 * Attempts to convert from a legacy UnserailizedSignableTransaction, into a
 * Programmable Transaction using the transaction builder. This should only be
 * used as a compatibility layer, and will be removed in a future release.
 *
 * @deprecated Use native `Transaction` instead, do not continue use of legacy transaction APIs.
 */
export function convertToTransactionBuilder({
  kind,
  data,
}: UnserializedSignableTransaction): Transaction {
  const tx = new Transaction();
  switch (kind) {
    case 'mergeCoin':
      tx.add(
        Commands.MergeCoins(tx.input(data.primaryCoin), [
          tx.input(data.coinToMerge),
        ]),
      );
      break;
    case 'paySui':
      data.recipients.forEach((recipient, index) => {
        const amount = data.amounts[index];
        const coin = tx.add(Commands.SplitCoin(tx.gas, tx.input(amount)));
        tx.add(Commands.TransferObjects(coin, tx.input(recipient)));
      });
      tx.setGasPayment(data.inputCoins);
      break;
    case 'transferObject':
      tx.add(
        Commands.TransferObjects(
          [tx.input(data.objectId)],
          tx.input(data.recipient),
        ),
      );
      break;
    case 'payAllSui':
      tx.add(Commands.TransferObjects([tx.gas], tx.input(data.recipient)));
      tx.setGasPayment(data.inputCoins);
      break;
    case 'splitCoin':
      data.splitAmounts.forEach((amount) => {
        tx.add(
          Commands.SplitCoin(tx.input(data.coinObjectId), tx.input(amount)),
        );
      });
      break;
    case 'moveCall':
      tx.add(
        Commands.MoveCall({
          package: data.packageObjectId,
          module: data.module,
          function: data.function,
          arguments: data.arguments.map((arg) => tx.input(arg)),
          typeArguments: data.typeArguments,
        }),
      );
      break;
    case 'publish':
      const modules = Array.from(data.compiledModules as ArrayLike<any>).map(
        (data: string | ArrayLike<number>) => [
          ...(typeof data === 'string' ? fromB64(data) : Array.from(data)),
        ],
      );
      tx.add(Commands.Publish(modules));
      break;
    case 'pay': {
      const [coin, ...coins] = data.inputCoins;
      const coinInput = tx.input(coin);
      if (coins.length > 0) {
        tx.add(
          Commands.MergeCoins(
            coinInput,
            coins.map((coin) => tx.input(coin)),
          ),
        );
      }
      data.recipients.forEach((recipient, index) => {
        const amount = data.amounts[index];
        const coin = tx.add(Commands.SplitCoin(coinInput, tx.input(amount)));
        tx.add(Commands.TransferObjects(coin, tx.input(recipient)));
      });
      break;
    }
    case 'transferSui': {
      const coin = tx.add(Commands.SplitCoin(tx.gas, tx.input(data.amount)));
      tx.add(Commands.TransferObjects(coin, tx.input(data.recipient)));
      tx.setGasPayment(data.suiObjectId);
      break;
    }
    default:
      throw new Error(`Unknown transaction kind: "${kind}"`);
  }

  if ('gasPayment' in data) {
    tx.setGasPayment(data.gasPayment);
  }
  if (data.gasBudget) {
    tx.setGasBudget(data.gasBudget);
  }
  if (data.gasPrice) {
    tx.setGasPrice(data.gasPrice);
  }

  return tx;
}
