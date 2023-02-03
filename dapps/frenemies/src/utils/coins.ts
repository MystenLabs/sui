// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Coin, SUI_COIN } from "../network/types";
import { ObjectData } from "../network/rawObject";
import { getTransactionEffects } from "@mysten/sui.js";
import { useWalletKit } from "@mysten/wallet-kit";
import { useMyType } from "../network/queries/use-raw";

export function getCoins(coins: ObjectData<Coin>[], amount: bigint) {
  const sorted = [...coins].sort((a, b) => Number(b.data.value - a.data.value));

  let sum = BigInt(0);
  let ret: ObjectData<Coin>[] = [];
  while (sum < amount) {
    const coin = sorted.pop();
    if (!coin) {
      throw new Error("Cannot find coins to meet amount.");
    }
    ret.push(coin);
    sum += coin.data.value;
  }
  return ret;
}

/**
 * Returns a Gas `ObjectData` if found or null;
 * Returns the rest of the Coins and their sum.
 */
export function getGas(coins: ObjectData<Coin>[], gasBudget: bigint) {
  const sorted = [...coins].sort((a, b) => Number(a.data.value - b.data.value));
  const gas = sorted.find((coin) => coin.data.value >= gasBudget) || null;

  if (gas === null) {
    return {
      gas: null,
      coins,
      max: 0n,
    };
  }

  const left = sorted.filter(
    (c) => c.reference.objectId !== gas.reference.objectId
  );
  const max = left.reduce((acc, c) => acc + c.data.value, 0n);

  return {
    gas,
    max,
    coins: left,
  };
}

export const DEFAULT_GAS_BUDGET_FOR_PAY = 150;

function computeGasBudgetForPay(
  coins: ObjectData<Coin>[],
  amountToSend: bigint
): number {
  const numInputCoins = getCoins(coins, amountToSend).length;

  return (
    DEFAULT_GAS_BUDGET_FOR_PAY * Math.max(2, Math.min(100, numInputCoins / 2))
  );
}

export function useManageCoin() {
  const { currentAccount, signAndExecuteTransaction } = useWalletKit();
  const { data: coins } = useMyType<Coin>(SUI_COIN, currentAccount);

  return async (amount: bigint, gasFee: bigint) => {
    if (!currentAccount) throw new Error("Missing account");
    if (!coins) throw new Error("No coins");

    const totalAmount = amount + gasFee;

    const inputCoins = getCoins(coins, totalAmount);

    const result = await signAndExecuteTransaction({
      kind: "paySui",
      data: {
        // NOTE: We reverse the order here so that the highest coin is in the front
        // so that it is used as the gas coin.
        inputCoins: [...inputCoins]
          .reverse()
          .map((coin) => coin.reference.objectId),
        recipients: [currentAccount, currentAccount],
        // TODO: Update SDK to accept bigint
        amounts: [Number(amount), Number(gasFee)],
        gasBudget: computeGasBudgetForPay(coins, totalAmount),
      },
    });

    const effects = getTransactionEffects(result);

    if (!effects || !effects.events) {
      throw new Error("Missing effects or events");
    }

    const changeEvent = effects.events.find((event) => {
      if ("coinBalanceChange" in event) {
        return event.coinBalanceChange.amount === Number(amount);
      }

      return false;
    });

    if (!changeEvent || !("coinBalanceChange" in changeEvent)) {
      throw new Error("Missing coin balance event");
    }

    return changeEvent.coinBalanceChange.coinObjectId;
  };
}
