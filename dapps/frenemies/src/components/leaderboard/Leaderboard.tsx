// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Record } from "./Row";
import Row from './Row';
import TableHeader from "./TableHeader";

type LeaderboardOptions = {
  rank: number,
  totalScore: number,
  records: Record[];
};

/**
 * Table representing a Leaderboard
 */
function Leaderboard({ records }: LeaderboardOptions) {
  return (
    <div className="w-auto">
      <table className="table-fixed w-auto">
        <TableHeader />
        {records.map((record) => {
          return <Row record={record} />
        })}
      </table>
    </div>
  );
}

export default Leaderboard;
