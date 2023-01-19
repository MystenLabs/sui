// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import Row from "./Row";
import TableHeader from "./TableHeader";
import { useLeaderboard } from "../../network/queries/leaderboard";
import { config } from "../../config";

/**
 * Table representing a Leaderboard
 */
function Leaderboard() {
  const { data } = useLeaderboard(config.VITE_LEADERBOARD);

  // TODO: Loading and error states:
  if (!data) {
    return null;
  }

  return (
    <div className="leaderboard w-auto">
      <table className="table-fixed w-auto">
        <TableHeader />
        {data.data.topScores.map((score) => (
          <Row score={score} />
        ))}
      </table>
    </div>
  );
}

export default Leaderboard;
