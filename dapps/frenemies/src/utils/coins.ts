// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Coin } from "../network/types";
import { ObjectData } from "../network/rawObject";

/**
 * Returns a Gas `ObjectData` if found or null;
 * Returns the rest of the Coins and their sum.
 */
export function getGas(
  coins: ObjectData<Coin>[],
  gasBudget: bigint
): { gas: ObjectData<Coin> | null; max: bigint; coins: ObjectData<Coin>[] } {
  const sorted = coins.sort((a, b) => Number(a.data.value - b.data.value));
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
    gas: gas,
    coins: left,
    max,
  };
}
