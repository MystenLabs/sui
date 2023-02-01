// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  CoinStruct,
  getTransactionEffects,
  SUI_TYPE_ARG,
} from "@mysten/sui.js";
import { useWalletKit } from "@mysten/wallet-kit";
import provider from "../network/provider";

export function useGetLatestCoins() {
  const { currentAccount } = useWalletKit();
  return async () => {
    if (!currentAccount) throw new Error("Wallet not connected");
    const { data } = await provider.getCoins(
      currentAccount,
      SUI_TYPE_ARG,
      undefined,
      1000
    );
    return data;
  };
}

export function getCoins(coins: CoinStruct[], amount: bigint) {
  const sorted = [...coins].sort((a, b) => Number(b.balance - a.balance));

  let sum = 0;
  let ret: CoinStruct[] = [];
  while (sum < amount) {
    const coin = sorted.pop();
    if (!coin) {
      throw new Error("Cannot find coins to meet amount.");
    }
    ret.push(coin);
    sum += coin.balance;
  }
  return ret;
}

/**
 * Returns a Gas `ObjectData` if found or null;
 * Returns the rest of the Coins and their sum.
 */
export function getGas(coins: CoinStruct[], gasBudget: bigint) {
  const sorted = [...coins].sort((a, b) => Number(a.balance - b.balance));
  const gas = sorted.find((coin) => coin.balance >= gasBudget) || null;

  if (gas === null) {
    return {
      gas: null,
      coins,
      max: 0n,
    };
  }

  const left = sorted.filter((c) => c.coinObjectId !== gas.coinObjectId);
  const max = BigInt(left.reduce((acc, c) => acc + c.balance, 0));

  return {
    gas,
    max,
    coins: left,
  };
}

export const DEFAULT_GAS_BUDGET_FOR_PAY = 150;

function computeGasBudgetForPay(
  coins: CoinStruct[],
  amountToSend: bigint
): number {
  const numInputCoins = getCoins(coins, amountToSend).length;

  return (
    DEFAULT_GAS_BUDGET_FOR_PAY * Math.max(2, Math.min(100, numInputCoins / 2))
  );
}

export function useManageCoin() {
  const { currentAccount, signAndExecuteTransaction } = useWalletKit();

  return async (coins: CoinStruct[], amount: bigint, gasFee: bigint) => {
    if (!currentAccount) throw new Error("Missing account");
    if (!coins.length) throw new Error("No coins");

    const totalAmount = amount + gasFee;

    const inputCoins = getCoins(coins, totalAmount);

    console.log(inputCoins, coins);

    const result = await signAndExecuteTransaction({
      kind: "paySui",
      data: {
        // NOTE: We reverse the order here so that the highest coin is in the front
        // so that it is used as the gas coin.
        inputCoins: [...inputCoins]
          .reverse()
          .map((coin) => coin.coinObjectId),
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
