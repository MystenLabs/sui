// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { UnserializedSignableTransaction } from '../signers/txn-data-serializers/txn-data-serializer';
import { Coin, SuiMoveObject } from '../types';

export type TxKind = UnserializedSignableTransaction['kind'];
type BudgetMap = typeof DEFAULT_GAS_BUDGET_PER_TX_TYPE;
export type GasBudgetGuessParams<txKind extends TxKind> =
  txKind extends keyof BudgetMap
    ? BudgetMap[txKind] extends (...args: any[]) => any
      ? Parameters<BudgetMap[txKind]> extends [boolean, ...infer Params]
        ? Params
        : []
      : []
    : [];

// The min-max were extracted by running a tx with too small (for min) and too big (for max) gasBudget
// TODO: consider getting them in a more dynamic way (from rpc maybe?)
/**
 * The minimum value accepted for gas budget
 */
export const MIN_GAS_BUDGET = 10;
/**
 * The maximum value accepted for gas budget
 */
export const MAX_GAS_BUDGET = 1_000_000;

/**
 * This constant is a 'guessed' estimation of the total cost of a pay tx with only one coin input.
 * We use this to calculate a 'guess' for the gas budget of a pay tx with multiple coins.
 */
const PAY_GAS_FEE_PER_COIN = 150;

/**
 * Make a guess for the gas budget of a pay tx
 * @param coins All the coins of the type to send
 * @param amount The amount to send
 * @returns The gasBudget guess
 */
function computeTransferTxGasBudget(
  optimizeGuessForDryRun: boolean,
  coins: SuiMoveObject[],
  amount: bigint
) {
  const numInputCoins = Coin.selectCoinSetWithCombinedBalanceGreaterThanOrEqual(
    coins,
    amount
  ).length;
  let gasBudgetGuess = Math.min(
    PAY_GAS_FEE_PER_COIN * Math.max(2, Math.min(100, numInputCoins / 2)),
    MAX_GAS_BUDGET
  );
  const isSuiTransfer = Coin.isSUI(coins[0]);
  if (isSuiTransfer && optimizeGuessForDryRun) {
    // check if there is enough balance to cover for the amount + the gas
    // if not lower the gasBudget to allow making better estimations for those cases
    const totalSuiBalance = coins.reduce(
      (sum, aCoin) => sum + (Coin.getBalance(aCoin) || BigInt(0)),
      BigInt(0)
    );
    if (totalSuiBalance - BigInt(gasBudgetGuess) - amount < 0) {
      gasBudgetGuess = Math.max(
        Number(totalSuiBalance - amount),
        MIN_GAS_BUDGET
      );
    }
  }
  return gasBudgetGuess;
}

const DEFAULT_GAS_BUDGET_PER_TX_TYPE = {
  mergeCoin: 10_000,
  moveCall: 10_000,
  pay: computeTransferTxGasBudget,
  paySui: computeTransferTxGasBudget,
  payAllSui: computeTransferTxGasBudget,
  publish: 10_000,
  splitCoin: 10_000,
  transferObject: 100,
  transferSui: 100,
} as const;

export function getGasBudgetGuess<T extends TxKind>(
  txKind: T,
  maxGasCoinBalance: bigint | number | null,
  optimizeGuessForDryRun: boolean,
  budgetParams: GasBudgetGuessParams<T>
): number {
  const gasBudgetGuess =
    typeof DEFAULT_GAS_BUDGET_PER_TX_TYPE[txKind] === 'function'
      ? // @ts-expect-error
        DEFAULT_GAS_BUDGET_PER_TX_TYPE[txKind](
          optimizeGuessForDryRun,
          ...budgetParams
        )
      : DEFAULT_GAS_BUDGET_PER_TX_TYPE[txKind];
  return Math.max(
    Math.min(
      ...[
        gasBudgetGuess,
        optimizeGuessForDryRun ? maxGasCoinBalance : null,
        MAX_GAS_BUDGET,
      ]
        .filter((aNum) => aNum !== null)
        .map((aNum) => Number(aNum))
    ),
    MIN_GAS_BUDGET
  );
}
