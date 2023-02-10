// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ReactNode } from "react";
import { config, ROUND_OFFSET } from "../../config";
import { useEpoch } from "../../network/queries/epoch";
import {
  useRefreshScorecard,
  useScorecard,
} from "../../network/queries/scorecard";
import { useRawObject } from "../../network/queries/use-raw";
import { Leaderboard, LEADERBOARD } from "../../network/types";

interface Props {
  fallback?: ReactNode;
}

export function Refresh({ fallback = null }: Props) {
  const { data: scorecard } = useScorecard();
  const { data: epoch } = useEpoch();
  const { data: leaderboard } = useRawObject<Leaderboard>(
    config.VITE_LEADERBOARD,
    LEADERBOARD
  );

  const refreshScorecard = useRefreshScorecard();

  if (
    !scorecard ||
    scorecard.data.assignment.epoch == epoch?.data.epoch ||
    !leaderboard
  ) {
    return <>{fallback}</>;
  }

  const round =
    BigInt(epoch?.epoch || 0) - leaderboard.data.startEpoch + ROUND_OFFSET ||
    0n;

  return (
    <div className="absolute top-0 right-0">
      <button
        className="bg-white shadow-button text-body font-semibold text-frenemies py-3 px-4 rounded-lg inline-flex items-center gap-2"
        onClick={() => {
          refreshScorecard.mutate();
        }}
      >
        <img src="/refresh.svg" alt="refresh" />
        Play Round {(round || 0).toString()}
      </button>
    </div>
  );
}
