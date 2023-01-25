// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useLeaderboard } from "../../network/queries/leaderboard";
import { config } from "../../config";
import { Stat } from "../Stat";
import { Table } from "./Table";

/**
 * Table representing a Leaderboard
 */
export function Leaderboard() {
  const { data } = useLeaderboard(
    config.VITE_LEADERBOARD || "0x7127db02f6313c03af19f7677b5155254dca8c52"
  );

  // TODO: Loading and error states:
  if (!data) {
    return null;
  }

  return (
    <>
      <div className="flex gap-16 mt-3 mb-7">
        <Stat variant="leaderboard" label="Highest Score">
          420
        </Stat>
        {/* <Stat variant="leaderboard" label="Total Score">
          420
        </Stat> */}
      </div>
      <Table data={data.data} />
    </>
  );
}
