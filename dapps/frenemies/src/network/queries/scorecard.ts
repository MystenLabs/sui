// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getRawObjectParsedUnsafe } from "../rawObject";
import { useQuery } from "@tanstack/react-query";
import { Scorecard } from "../types";
import provider from "../provider";

/**
 * Type signature for the Scorecard type.
 * TODO: Ideally should include the packageID.
 */
const SCORECARD_TYPE = "frenemies::Scorecard";

/**
 * Get a Scorecard for an account if this account has at least one.
 *
 * We do not guarantee correct behavior if people registered more than once,
 * lookup is done with `Array.prototype.find` for the first occurrence.
 */
export function useScorecard(account?: string | null) {
  return useQuery(
    ["scorecard", account],
    async () => {
      if (!account) {
        return null;
      }

      const objects = await provider.getObjectsOwnedByAddress(account);
      const search = objects.find((v) => v.type.includes(SCORECARD_TYPE));

      if (!search) {
        return null;
      }

      return getRawObjectParsedUnsafe<Scorecard>(
        provider,
        search.objectId,
        "frenemies::Scorecard"
      );
    },
    {
      enabled: !!account,
    }
  );
}
