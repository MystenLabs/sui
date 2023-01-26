// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Coin } from "../network/types";
import { ObjectData } from "../network/rawObject";

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
  const sorted = [...coins].sort((a, b) => Number(b.data.value - a.data.value));
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
