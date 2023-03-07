// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Provider } from '../providers/provider';
import { LocalTxnDataSerializer } from '../signers/txn-data-serializers/local-txn-data-serializer';
import { UnserializedSignableTransaction } from '../signers/txn-data-serializers/txn-data-serializer';
import { TypeTagSerializer } from '../signers/txn-data-serializers/type-tag-serializer';
import { getObjectReference, getObjectType, SUI_TYPE_ARG } from '../types';
import { Transaction, Commands } from './';

/**
 * Attempts to convert from a legacy UnserailizedSignableTransaction, into a
 * Programmable Transaction using the transaction builder. This should only be
 * used as a compatibility layer, and will be removed in a future release.
 *
 * @deprecated Use native `Transaction` instead, do not continue use of legacy transaction APIs.
 */
export async function convertToTransactionBuilder(
  sender: string,
  { kind, data }: UnserializedSignableTransaction,
  provider: Provider,
) {
  const tx = new Transaction();
  tx.setSender(sender);
  switch (kind) {
    case 'mergeCoin': {
      const coinToMerge = await provider.getObject(data.coinToMerge, {
        showType: true,
      });

      if (getObjectType(coinToMerge) === '0x2::coin::Coin<0x2::sui::SUI>') {
        // Merging Sui, need to avoid gas overlap:
        const coins = await provider.getCoins(sender, SUI_TYPE_ARG, null, null);
        tx.add(
          Commands.MergeCoins(tx.input(data.primaryCoin), [
            tx.input(data.coinToMerge),
          ]),
        );
        tx.setGasPayment(
          coins.data
            .filter(
              (coin) =>
                coin.coinObjectId !== data.primaryCoin &&
                coin.coinObjectId !== data.coinToMerge,
            )
            .map((coin) => ({
              objectId: coin.coinObjectId,
              digest: coin.digest,
              version: coin.version,
            })),
        );
      } else {
        tx.add(
          Commands.MergeCoins(tx.input(data.primaryCoin), [
            tx.input(data.coinToMerge),
          ]),
        );
      }
      break;
    }
    case 'paySui': {
      data.recipients.forEach((recipient, index) => {
        const amount = data.amounts[index];
        const coin = tx.add(Commands.SplitCoin(tx.gas, tx.input(amount)));
        tx.add(Commands.TransferObjects([coin], tx.input(recipient)));
      });
      const objects = await provider.getObjectBatch(data.inputCoins, {
        showOwner: true,
      });
      tx.setGasPayment(objects.map((obj) => getObjectReference(obj)!));
      break;
    }
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
      const objects = await provider.getObjectBatch(data.inputCoins, {
        showOwner: true,
      });
      tx.setGasPayment(objects.map((obj) => getObjectReference(obj)!));
      break;
    case 'splitCoin': {
      const coinToSplit = await provider.getObject(data.coinObjectId, {
        showType: true,
      });

      if (getObjectType(coinToSplit) === '0x2::coin::Coin<0x2::sui::SUI>') {
        // For Sui amounts, we'll just split off the gas, to avoid overlapping
        // gas and input
        data.splitAmounts.forEach((amount) => {
          const coin = tx.add(Commands.SplitCoin(tx.gas, tx.input(amount)));
          tx.add(Commands.TransferObjects([coin], tx.input(sender)));
        });
        tx.setGasPayment([getObjectReference(coinToSplit)!]);
      } else {
        const splitCoinInput = tx.input(data.coinObjectId);
        data.splitAmounts.forEach((amount) => {
          const coin = tx.add(
            Commands.SplitCoin(splitCoinInput, tx.input(amount)),
          );
          tx.add(Commands.TransferObjects([coin], tx.input(sender)));
        });
      }
      break;
    }
    case 'moveCall':
      tx.add(
        Commands.MoveCall({
          package: data.packageObjectId,
          module: data.module,
          function: data.function,
          arguments: data.arguments.map((arg) => tx.input(arg)),
          typeArguments: data.typeArguments.map((arg) =>
            typeof arg === 'string' ? arg : TypeTagSerializer.tagToString(arg),
          ),
        }),
      );
      break;
    case 'publish': {
      // TODO: Fix publish transactions:
      const serializer = new LocalTxnDataSerializer(provider);
      return await serializer.serializeToBytes(
        sender,
        { kind: 'publish', data },
        'Commit',
      );
      // const modules = Array.from(data.compiledModules as ArrayLike<any>).map(
      //   (data: string | ArrayLike<number>) => [
      //     ...(typeof data === 'string' ? fromB64(data) : Array.from(data)),
      //   ],
      // );
      // tx.add(Commands.Publish(modules));
      // break;
    }
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
        tx.add(Commands.TransferObjects([coin], tx.input(recipient)));
      });
      break;
    }
    case 'transferSui': {
      const coin = tx.add(Commands.SplitCoin(tx.gas, tx.input(data.amount)));
      tx.add(Commands.TransferObjects([coin], tx.input(data.recipient)));
      const object = await provider.getObject(data.suiObjectId);
      tx.setGasPayment([getObjectReference(object)!]);
      break;
    }
    default:
      throw new Error(`Unknown transaction kind: "${kind}"`);
  }

  if ('gasPayment' in data && data.gasPayment) {
    const object = await provider.getObject(data.gasPayment);
    tx.setGasPayment([getObjectReference(object)!]);
  }
  if (data.gasBudget) {
    tx.setGasBudget(data.gasBudget);
  }
  if (data.gasPrice) {
    tx.setGasPrice(data.gasPrice);
  }

  return tx.build({ provider });
}
