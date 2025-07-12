// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React from "react";
import { useHits, Pagination } from "react-instantsearch";

export default function ConditionalPagination() {
  const { hits } = useHits();
  if (hits.length === 0) return null;
  return <Pagination />;
}
