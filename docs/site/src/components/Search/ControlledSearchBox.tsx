// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React from "react";
import { useSearchBox } from "react-instantsearch";

export default function ControlledSearchBox({
  placeholder,
  query,
  onChange,
}: {
  placeholder: string;
  query: string;
  onChange: (value: string) => void;
}) {
  const { refine } = useSearchBox();

  React.useEffect(() => {
    refine(query);
  }, [query, refine]);

  React.useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    if (query) {
      params.set("q", query);
    } else {
      params.delete("q");
    }
    const newUrl = `${window.location.pathname}?${params.toString()}`;
    window.history.replaceState(null, "", newUrl);
  }, [query]);

  return (
    <input
      type="search"
      className="w-full border border-sui-gray-40 dark:border-sui-gray-70 rounded bg-sui-gray-50 dark:bg-sui-gray-80 px-4 py-2 h-12 mb-8 text-lg"
      placeholder={placeholder}
      value={query}
      onChange={(e) => onChange(e.currentTarget.value)}
    />
  );
}
