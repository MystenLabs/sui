// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect, useState } from "react";
import { useEpoch } from "../network/queries/epoch";
import { formatTimeRemaining } from "../utils/format";
import { Stat } from "./Stat";


/**
 * Calculate time left until the next epoch based on the last two timestamps
 * for epoch changes (`timestamp` and `prevTimestamp`).
 */
function getTime(timestamp?: number, prevTimestamp?: number): number | null {
  if (!timestamp || !prevTimestamp) return null;

  const prevEpochLength = timestamp - prevTimestamp;
  const timePassed = Date.now() - timestamp;
  const timeLeft = prevEpochLength - timePassed;
  return timeLeft <= 0 ? 0 : timeLeft;
}

export function TimeRemaining() {
	const { data: epoch } = useEpoch();

	const [timer, setTime] = useState(() =>
    getTime(epoch?.timestamp, epoch?.prevTimestamp)
  );

  useEffect(() => {
    if (!epoch) return;

    const interval = setInterval(
      () => setTime(getTime(epoch.timestamp, epoch.prevTimestamp)),
      1000
    );
    return () => clearInterval(interval);
  }, [epoch]);

  return (
    <Stat label="Time Remaining">
      <div className="text-steel-dark font-light">
        {formatTimeRemaining(timer || 0)}
      </div>
    </Stat>
  );
}
