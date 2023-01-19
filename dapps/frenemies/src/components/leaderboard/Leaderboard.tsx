// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import Row from './Row';
import TableHeader from "./TableHeader";
import type { Leaderboard as LeaderboardType } from '../../network/types';

/**
 * Table representing a Leaderboard
 */
function Leaderboard({ board }: { board: LeaderboardType}) {
  return (
    <div className="leaderboard w-auto">
      <table className="table-fixed w-auto">
        <TableHeader />
        {board.topScores.map((score) => <Row score={score} />)}
      </table>
    </div>
  );
}

export default Leaderboard;
