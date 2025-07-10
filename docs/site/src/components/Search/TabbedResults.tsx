// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React from "react";

export default function TabbedResults({ activeTab, onChange, tabs }) {
  const suitooltip = "Search results from the official Sui Docs";
  const suinstooltip = "Search results from Sui Name Service";
  const movetooltip = "Search results from The Move Book";
  const dapptooltip = "Search results from the Sui dApp Kit (TypeScript SDK)";
  return (
    <div className="mb-4 flex justify-start border-2 border-solid border-white dark:border-sui-black border-b-sui-gray-50 dark:border-b-sui-gray-80">
      {tabs.map(({ label, indexName, count }) => (
        <div className="relative group inline-block" key={indexName}>
          <button
            className={`mr-4 text-sm lg:text-md xl:text-lg bg-white dark:bg-sui-black cursor-pointer ${activeTab === indexName ? "text-sui-blue border-2 border-solid border-transparent border-b-sui-blue-dark dark:border-b-sui-blue" : "border-transparent"}`}
            onClick={() => onChange(indexName)}
          >
            {label}{" "}
            <span className="text-xs bg-sui-gray-50 dark:bg-sui-gray-80 rounded-full py-1 px-2">
              {count}
            </span>
          </button>
          <div className="absolute bottom-full left-1/2 -translate-x-1/2 mb-2 w-max max-w-xs px-2 py-1 text-sm text-white bg-gray-800 rounded tooltip-delay">
            {label === "Sui"
              ? suitooltip
              : label === "SuiNS"
                ? suinstooltip
                : label === "The Move Book and Reference"
                  ? movetooltip
                  : dapptooltip}
          </div>
        </div>
      ))}
    </div>
  );
}
