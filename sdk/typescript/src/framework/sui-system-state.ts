// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { TransactionBlock } from '../builder';
import { JsonRpcProvider } from '../providers/json-rpc-provider';
import {
  getObjectReference,
  normalizeSuiObjectId,
  ObjectId,
  SuiAddress,
  SUI_SYSTEM_ADDRESS,
} from '../types';

/**
 * Address of the Sui System object.
 * Always the same in every Sui network (local, devnet, testnet).
 */
export const SUI_SYSTEM_STATE_OBJECT_ID: string = normalizeSuiObjectId('0x5');

export const SUI_SYSTEM_MODULE_NAME = 'sui_system';
export const ADD_STAKE_FUN_NAME = 'request_add_stake';
export const ADD_STAKE_LOCKED_COIN_FUN_NAME =
  'request_add_stake_with_locked_coin';
export const WITHDRAW_STAKE_FUN_NAME = 'request_withdraw_stake';

/**
 * Utility class for `0x5` object
 */
export class SuiSystemStateUtil {
  /**
   * Create a new transaction for staking coins ready to be signed and executed with `signer-and-provider`.
   *
   * @param coins the coins to be staked
   * @param amount the amount to stake
   * @param gasBudget omittable only for DevInspect mode
   */
  public static async newRequestAddStakeTxn(
    provider: JsonRpcProvider,
    coins: ObjectId[],
    amount: bigint,
    validatorAddress: SuiAddress,
  ): Promise<TransactionBlock> {
    // TODO: validate coin types and handle locked coins
    const tx = new TransactionBlock();

    const coin = tx.splitCoins(tx.gas, [tx.pure(amount)]);
    tx.moveCall({
      target: `${SUI_SYSTEM_ADDRESS}::${SUI_SYSTEM_MODULE_NAME}::${ADD_STAKE_FUN_NAME}`,
      arguments: [
        tx.object(SUI_SYSTEM_STATE_OBJECT_ID),
        coin,
        tx.pure(validatorAddress),
      ],
    });
    const coinObjects = await provider.multiGetObjects({
      ids: coins,
      options: {
        showOwner: true,
      },
    });
    tx.setGasPayment(coinObjects.map((obj) => getObjectReference(obj)!));
    return tx;
  }

  /**
   * Create a new transaction for withdrawing coins ready to be signed and
   * executed with `signer-and-provider`.
   *
   * @param stake the stake object created in the requestAddStake txn
   * @param stakedCoinId the coins to withdraw
   * @param gasBudget omittable only for DevInspect mode
   */
  public static async newRequestWithdrawlStakeTxn(
    stake: ObjectId,
    stakedCoinId: ObjectId,
  ): Promise<TransactionBlock> {
    const tx = new TransactionBlock();
    tx.moveCall({
      target: `${SUI_SYSTEM_ADDRESS}::${SUI_SYSTEM_MODULE_NAME}::${WITHDRAW_STAKE_FUN_NAME}`,
      arguments: [
        tx.object(SUI_SYSTEM_STATE_OBJECT_ID),
        tx.object(stake),
        tx.object(stakedCoinId),
      ],
    });

    return tx;
  }
}
