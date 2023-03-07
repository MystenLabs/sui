// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Commands, Transaction } from '../builder';
import { Provider } from '../providers/provider';
import {
  getObjectReference,
  normalizeSuiAddress,
  ObjectId,
  SuiAddress,
  SUI_FRAMEWORK_ADDRESS,
} from '../types';

/**
 * Address of the Sui System object.
 * Always the same in every Sui network (local, devnet, testnet).
 */
export const SUI_SYSTEM_STATE_OBJECT_ID: string = normalizeSuiAddress('0x5');

export const SUI_SYSTEM_MODULE_NAME = 'sui_system';
export const ADD_DELEGATION_FUN_NAME = 'request_add_delegation';
export const ADD_DELEGATION_LOCKED_COIN_FUN_NAME =
  'request_add_delegation_with_locked_coin';
export const WITHDRAW_DELEGATION_FUN_NAME = 'request_withdraw_delegation';

/**
 * Utility class for `0x5` object
 */
export class SuiSystemStateUtil {
  /**
   * Create a new transaction for delegating coins ready to be signed and executed with `signer-and-provider`.
   *
   * @param coins the coins to be used in delegation
   * @param amount the amount to delegate
   * @param gasBudget omittable only for DevInspect mode
   */
  public static async newRequestAddDelegationTxn(
    provider: Provider,
    coins: ObjectId[],
    amount: bigint,
    validatorAddress: SuiAddress,
  ): Promise<Transaction> {
    // TODO: validate coin types and handle locked coins
    const tx = new Transaction();
    const coin = tx.add(Commands.SplitCoin(tx.gas, tx.input(amount)));
    tx.add(
      Commands.MoveCall({
        package: SUI_FRAMEWORK_ADDRESS,
        module: SUI_SYSTEM_MODULE_NAME,
        function: ADD_DELEGATION_FUN_NAME,
        typeArguments: [],
        arguments: [
          tx.input(SUI_SYSTEM_STATE_OBJECT_ID),
          coin,
          tx.input(validatorAddress),
        ],
      }),
    );
    const coinObjects = await provider.getObjectBatch(coins, {
      showOwner: true,
    });
    tx.setGasPayment(coinObjects.map((obj) => getObjectReference(obj)!));
    return tx;
  }

  /**
   * Create a new transaction for withdrawing coins ready to be signed and
   * executed with `signer-and-provider`.
   *
   * @param delegation the delegation object created in the requestAddDelegation txn
   * @param stakedCoinId the coins to withdraw
   * @param gasBudget omittable only for DevInspect mode
   */
  public static async newRequestWithdrawlDelegationTxn(
    delegation: ObjectId,
    stakedCoinId: ObjectId,
  ): Promise<Transaction> {
    const tx = new Transaction();
    tx.add(
      Commands.MoveCall({
        package: SUI_FRAMEWORK_ADDRESS,
        module: SUI_SYSTEM_MODULE_NAME,
        function: WITHDRAW_DELEGATION_FUN_NAME,
        typeArguments: [],
        arguments: [
          tx.input(SUI_SYSTEM_STATE_OBJECT_ID),
          tx.input(delegation),
          tx.input(stakedCoinId),
        ],
      }),
    );
    return tx;
  }
}
