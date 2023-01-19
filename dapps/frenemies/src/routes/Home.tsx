// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import Round from "../components/round/Round";
import Leaderboard from "../components/leaderboard/Leaderboard";
import Block from "../components/block/Block";

/**
 * The Home page.
 */
export function Home() {
  return (
    <>
      <Leaderboard />
      <Round num={10} />
      <div className="flex content-center my-5">
        <Block title="Your role" value="Friend" />
        <Block title="Assigned Validator" value="ValidatorA" />
        <Block title="Time Remaining" value="10:14:42" />
      </div>
      <div className="card my-10">{/* Add current status */}</div>
      <div className="card my-10">
        {/* Add staking page (mark the staking goal) */}
      </div>
    </>
  );
}
