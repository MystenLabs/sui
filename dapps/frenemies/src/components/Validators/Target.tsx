// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Goal } from "../../network/types";

interface Props {
  goal: Goal;
}

const GOAL_TO_COPY = {
  [Goal.Friend]: "Friend, Goal 1-13 Rank",
  [Goal.Neutral]: "Neutral, Goal 14-25 Rank",
  [Goal.Enemy]: "Enemy, Goal 26-41 Rank",
};

export function Target({ goal }: Props) {
  return <div className="text-sm text-left">{GOAL_TO_COPY[goal]}</div>;
}
