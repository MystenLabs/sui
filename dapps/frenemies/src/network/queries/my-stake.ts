// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useQuery } from "@tanstack/react-query";
import { getRawObjectParsedUnsafe } from "../rawObject";
import { StakedSui } from "./../types";
import provider from "../provider";

/**
 * Type signature for the Scorecard type.
 * TODO: Ideally should include the packageID.
 */
const STAKED_SUI = "staking_pool::StakedSui";

/**
 * Get a Scorecard for an account if this account has at least one.
 *
 * We do not guarantee correct behavior if people registered more than once,
 * lookup is done with `Array.prototype.find` for the first occurrence.
 */
export function useMyStake(account: string | null) {
  return useQuery(
    ["my-stake", account],
    async () => {
      if (!account) {
        return null;
      }

      const objects = await provider.getObjectsOwnedByAddress(account);
      const search = objects.filter((v) => v.type.includes(STAKED_SUI));

      if (!search) {
        return null;
      }

      return Promise.all(
        search.map((obj) => getRawObjectParsedUnsafe<StakedSui>(provider, obj.objectId, "staking_pool::StakedSui"))
      );
    }
  );
}
