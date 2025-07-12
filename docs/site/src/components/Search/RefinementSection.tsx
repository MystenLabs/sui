// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React from "react";
import { RefinementList, useHits } from "react-instantsearch";

export default function RefinementSection() {
  const { hits } = useHits();
  if (hits.length === 0) return null;
  return (
    <div className="col-span-12 md:col-span-4 xl:col-span-3">
      <div className="sticky p-4 top-24 z-10 bg-sui-gray-50 dark:bg-sui-gray-80">
        <h2 className="text-lg">Refine results</h2>
        <RefinementList attribute="source" />
      </div>
    </div>
  );
}
