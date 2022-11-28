// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  getObjectFields,
  GetObjectDataResponse,
  SuiMoveObject,
  SuiObjectInfo,
  SuiObject,
  SuiData,
  getMoveObjectType,
  ObjectId,
  getObjectId,
} from './objects';
import { normalizeSuiObjectId, SuiAddress } from './common';

import { getOption, Option } from './option';
import { StructTag } from './sui-bcs';
import { isSuiMoveObject } from './index.guard';
import { SignerWithProvider } from '../signers/signer-with-provider';

export const SUI_FRAMEWORK_ADDRESS = '0x2';
export const MOVE_STDLIB_ADDRESS = '0x1';
export const OBJECT_MODULE_NAME = 'object';
export const UID_STRUCT_NAME = 'UID';
export const ID_STRUCT_NAME = 'ID';
export const SUI_TYPE_ARG = `${SUI_FRAMEWORK_ADDRESS}::sui::SUI`;

export const COIN_TYPE = `${SUI_FRAMEWORK_ADDRESS}::coin::Coin`;

// `sui::pay` module is used for Coin management (split, join, join_and_transfer etc);
export const PAY_MODULE_NAME = 'pay';
export const PAY_SPLIT_COIN_VEC_FUNC_NAME = 'split_vec';
export const PAY_JOIN_COIN_FUNC_NAME = 'join';
export const COIN_TYPE_ARG_REGEX = /^0x2::coin::Coin<(.+)>$/;

type ObjectData = ObjectDataFull | SuiObjectInfo;
type ObjectDataFull = GetObjectDataResponse | SuiMoveObject;

export type CoinMetadata = {
  decimals: number;
  name: string;
  symbol: string;
  description: string;
  iconUrl: string | null;
  id: ObjectId | null;
};

/**
 * Utility class for 0x2::coin
 * as defined in https://github.com/MystenLabs/sui/blob/ca9046fd8b1a9e8634a4b74b0e7dabdc7ea54475/sui_programmability/framework/sources/Coin.move#L4
 */
export class Coin {
  static isCoin(data: ObjectData): boolean {
    return Coin.getType(data)?.startsWith(COIN_TYPE) ?? false;
  }

  static getCoinType(type: string) {
    const [, res] = type.match(COIN_TYPE_ARG_REGEX) ?? [];
    return res || null;
  }

  static getCoinTypeArg(obj: ObjectData) {
    const type = Coin.getType(obj);
    return type ? Coin.getCoinType(type) : null;
  }

  static isSUI(obj: ObjectData) {
    const arg = Coin.getCoinTypeArg(obj);
    return arg ? Coin.getCoinSymbol(arg) === 'SUI' : false;
  }

  static getCoinSymbol(coinTypeArg: string) {
    return coinTypeArg.substring(coinTypeArg.lastIndexOf(':') + 1);
  }

  static getCoinStructTag(coinTypeArg: string): StructTag {
    return {
      address: normalizeSuiObjectId(coinTypeArg.split('::')[0]),
      module: coinTypeArg.split('::')[1],
      name: coinTypeArg.split('::')[2],
      typeParams: [],
    };
  }

  public static getID(obj: ObjectData): ObjectId {
    if (isSuiMoveObject(obj)) {
      return obj.fields.id.id;
    }
    return getObjectId(obj);
  }

  /**
   * Convenience method for select coin objects that has a balance greater than or equal to `amount`
   *
   * @param amount coin balance
   * @param exclude object ids of the coins to exclude
   * @return a list of coin objects that has balance greater than `amount` in an ascending order
   */
  static selectCoinsWithBalanceGreaterThanOrEqual(
    coins: ObjectDataFull[],
    amount: bigint,
    exclude: ObjectId[] = []
  ): ObjectDataFull[] {
    return Coin.sortByBalance(
      coins.filter(
        (c) => !exclude.includes(Coin.getID(c)) && Coin.getBalance(c)! >= amount
      )
    );
  }

  /**
   * Convenience method for select an arbitrary coin object that has a balance greater than or
   * equal to `amount`
   *
   * @param amount coin balance
   * @param exclude object ids of the coins to exclude
   * @return an arbitray coin with balance greater than or equal to `amount
   */
  static selectCoinWithBalanceGreaterThanOrEqual(
    coins: ObjectDataFull[],
    amount: bigint,
    exclude: ObjectId[] = []
  ): ObjectDataFull | undefined {
    return coins.find(
      (c) => !exclude.includes(Coin.getID(c)) && Coin.getBalance(c)! >= amount
    );
  }

  /**
   * Convenience method for select a minimal set of coin objects that has a balance greater than
   * or equal to `amount`. The output can be used for `PayTransaction`
   *
   * @param amount coin balance
   * @param exclude object ids of the coins to exclude
   * @return a minimal list of coin objects that has a combined balance greater than or equal
   * to`amount` in an ascending order. If no such set exists, an empty list is returned
   */
  static selectCoinSetWithCombinedBalanceGreaterThanOrEqual(
    coins: ObjectDataFull[],
    amount: bigint,
    exclude: ObjectId[] = []
  ): ObjectDataFull[] {
    const sortedCoins = Coin.sortByBalance(
      coins.filter((c) => !exclude.includes(Coin.getID(c)))
    );

    const total = Coin.totalBalance(sortedCoins);
    // return empty set if the aggregate balance of all coins is smaller than amount
    if (total < amount) {
      return [];
    } else if (total === amount) {
      return sortedCoins;
    }

    let sum = BigInt(0);
    let ret = [];
    while (sum < total) {
      // prefer to add a coin with smallest sufficient balance
      const target = amount - sum;
      const coinWithSmallestSufficientBalance = sortedCoins.find(
        (c) => Coin.getBalance(c)! >= target
      );
      if (coinWithSmallestSufficientBalance) {
        ret.push(coinWithSmallestSufficientBalance);
        break;
      }

      const coinWithLargestBalance = sortedCoins.pop()!;
      ret.push(coinWithLargestBalance);
      sum += Coin.getBalance(coinWithLargestBalance)!;
    }

    return Coin.sortByBalance(ret);
  }

  static totalBalance(coins: ObjectDataFull[]): bigint {
    return coins.reduce(
      (partialSum, c) => partialSum + Coin.getBalance(c)!,
      BigInt(0)
    );
  }

  /**
   * Sort coin by balance in an ascending order
   */
  static sortByBalance(coins: ObjectDataFull[]): ObjectDataFull[] {
    return coins.sort((a, b) =>
      Coin.getBalance(a)! < Coin.getBalance(b)!
        ? -1
        : Coin.getBalance(a)! > Coin.getBalance(b)!
        ? 1
        : 0
    );
  }

  static getBalance(data: ObjectDataFull): bigint | undefined {
    if (!Coin.isCoin(data)) {
      return undefined;
    }
    const balance = getObjectFields(data)?.balance;
    return BigInt(balance);
  }

  static getZero(): bigint {
    return BigInt(0);
  }

  private static getType(data: ObjectData): string | undefined {
    if ('status' in data) {
      return getMoveObjectType(data);
    }
    return data.type;
  }

  /**
   * Creates and executes a transaction to transfer coins to a recipient address.
   * @param signer User's signer
   * @param allCoins All the coins that are owned by the user. Can be only the relevant type of coins for the transfer, Sui for gas and the coins with the same type as the type to send.
   * @param coinTypeArg The coin type argument (Coin<T> the T) of the coin to send
   * @param amountToSend Total amount to send to recipient
   * @param recipient Recipient's address
   * @param gasBudget Gas budget for the tx
   * @throws in case of insufficient funds, network errors etc
   */
  public static async transfer(
    signer: SignerWithProvider,
    allCoins: SuiMoveObject[],
    coinTypeArg: string,
    amountToSend: bigint,
    recipient: SuiAddress,
    gasBudget: number
  ) {
    const tx = await Coin.newTransferTx(
      signer,
      allCoins,
      coinTypeArg,
      amountToSend,
      recipient,
      gasBudget
    );
    return signer.signAndExecuteTransaction(tx);
  }

  /**
   * Create a new transaction for sending coins ready to be signed and executed.
   * @param signer User's signer
   * @param allCoins All the coins that are owned by the user. Can be only the relevant type of coins for the transfer, Sui for gas and the coins with the same type as the type to send.
   * @param coinTypeArg The coin type argument (Coin<T> the T) of the coin to send
   * @param amountToSend Total amount to send to recipient
   * @param recipient Recipient's address
   * @param gasBudget Gas budget for the tx
   * @throws in case of insufficient funds, network errors etc
   */
  private static async newTransferTx(
    signer: SignerWithProvider,
    allCoins: SuiMoveObject[],
    coinTypeArg: string,
    amountToSend: bigint,
    recipient: SuiAddress,
    gasBudget: number
  ) {
    const isSuiTransfer = coinTypeArg === SUI_TYPE_ARG;
    const coinsOfTransferType = allCoins.filter(
      (aCoin) => Coin.getCoinTypeArg(aCoin) === coinTypeArg
    );
    const totalAmountIncludingGas =
      amountToSend + BigInt(isSuiTransfer ? gasBudget : 0);
    const inputCoinObjs =
      await Coin.selectCoinSetWithCombinedBalanceGreaterThanOrEqual(
        coinsOfTransferType,
        totalAmountIncludingGas
      );
    if (!inputCoinObjs.length) {
      const totalBalanceOfTransferType = Coin.totalBalance(coinsOfTransferType);
      const suggestedAmountToSend =
        totalBalanceOfTransferType - BigInt(isSuiTransfer ? gasBudget : 0);
      // TODO: denomination for values?
      throw new Error(
        `Coin balance ${totalBalanceOfTransferType} is not sufficient to cover the transfer amount ` +
          `${amountToSend}. Try reducing the transfer amount to ${suggestedAmountToSend}.`
      );
    }
    if (!isSuiTransfer) {
      const allGasCoins = allCoins.filter((aCoin) => Coin.isSUI(aCoin));
      const gasCoin = Coin.selectCoinWithBalanceGreaterThanOrEqual(
        allGasCoins,
        BigInt(gasBudget)
      );
      if (!gasCoin) {
        // TODO: denomination for gasBudget?
        throw new Error(
          `Unable to find a coin to cover the gas budget ${gasBudget}`
        );
      }
    }
    const signerAddress = await signer.getAddress();
    const inputCoins = inputCoinObjs.map(Coin.getID);
    const txCommon = {
      inputCoins,
      recipients: [recipient],
      // TODO: change this to string to avoid losing precision
      amounts: [Number(amountToSend)],
      gasBudget: Number(gasBudget),
    };
    if (isSuiTransfer) {
      return signer.serializer.newPaySui(signerAddress, {
        ...txCommon,
      });
    }
    return signer.serializer.newPay(signerAddress, {
      ...txCommon,
      // we know there is a gas coin with enough balance to cover
      // the gas budget let rpc select one for us
    });
  }
}

export type DelegationData = SuiMoveObject &
  Pick<SuiData, 'dataType'> & {
    type: '0x2::delegation::Delegation';
    fields: {
      active_delegation: Option<number>;
      delegate_amount: number;
      next_reward_unclaimed_epoch: number;
      validator_address: SuiAddress;
      info: {
        id: string;
        version: number;
      };
      coin_locked_until_epoch: Option<SuiMoveObject>;
      ending_epoch: Option<number>;
    };
  };

export type DelegationSuiObject = Omit<SuiObject, 'data'> & {
  data: DelegationData;
};

// Class for delegation.move
// see https://github.com/MystenLabs/fastnft/blob/161aa27fe7eb8ecf2866ec9eb192e768f25da768/crates/sui-framework/sources/governance/delegation.move
export class Delegation {
  public static readonly SUI_OBJECT_TYPE = '0x2::delegation::Delegation';
  private suiObject: DelegationSuiObject;

  public static isDelegationSuiObject(
    obj: SuiObject
  ): obj is DelegationSuiObject {
    return 'type' in obj.data && obj.data.type === Delegation.SUI_OBJECT_TYPE;
  }

  constructor(obj: DelegationSuiObject) {
    this.suiObject = obj;
  }

  public nextRewardUnclaimedEpoch() {
    return this.suiObject.data.fields.next_reward_unclaimed_epoch;
  }

  public activeDelegation() {
    return BigInt(getOption(this.suiObject.data.fields.active_delegation) || 0);
  }

  public delegateAmount() {
    return this.suiObject.data.fields.delegate_amount;
  }

  public endingEpoch() {
    return getOption(this.suiObject.data.fields.ending_epoch);
  }

  public validatorAddress() {
    return this.suiObject.data.fields.validator_address;
  }

  public isActive() {
    return this.activeDelegation() > 0 && !this.endingEpoch();
  }

  public hasUnclaimedRewards(epoch: number) {
    return (
      this.nextRewardUnclaimedEpoch() <= epoch &&
      (this.isActive() || (this.endingEpoch() || 0) > epoch)
    );
  }
}
