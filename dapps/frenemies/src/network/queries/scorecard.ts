// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Scorecard } from "../types";
import { getRawObject, ObjectData } from "../rawObject";
import provider from "../provider";
import bcs from "../bcs";
import { useQuery } from "@tanstack/react-query";

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
export function useScorecard(account: string) {
  return useQuery(
    ["scorecard", account],
    async (): Promise<ObjectData<Scorecard> | null> => {
      const objects = await provider.getObjectsOwnedByAddress(account);
      const search = objects.find((v) => v.type.includes(SCORECARD_TYPE));

      if (!search) {
        return null;
      }

      const objectData = await getRawObject(provider, search.objectId);
      const {
        reference,
        data: { bcs_bytes },
      } = objectData.details;

      return {
        reference,
        data: bcs.de("frenemies::Scorecard", bcs_bytes, "base64"),
      };
    }
  );
}
