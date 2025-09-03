// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React from "react";
import { useStats } from "react-instantsearch";

export default function IndexStatsCollector({
  indexName,
  onUpdate,
}: {
  indexName: string;
  onUpdate: (indexName: string, hits: number) => void;
}) {
  const { nbHits } = useStats();
  React.useEffect(() => {
    onUpdate(indexName, nbHits);
  }, [indexName, nbHits, onUpdate]);
  return null;
}
