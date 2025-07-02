// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React from "react";
import { liteClient as algoliasearch } from "algoliasearch/lite";
import { InstantSearch, Index } from "react-instantsearch";

import ControlledSearchBox from "./ControlledSearchBox";
import TabbedResults from "./TabbedResults";
import IndexStatsCollector from "./IndexStatsCollector";
import TabbedIndex from "./TabbedIndex";

function getQueryParam(key) {
  const params = new URLSearchParams(
    typeof window !== "undefined" ? window.location.search : "",
  );
  return params.get(key) || "";
}

function TabbedResults({ activeTab, onChange, tabs }) {
  const suitooltip = "Search results from the official Sui Docs";
  const suinstooltip = "Search results from Sui Name Service";
  const movetooltip = "Search results from The Move Book";
  const dapptooltip = "Search results from the Sui dApp Kit (TypeScript SDK)";
  return (
    <div className="mb-4 flex justify-start border-2 border-solid border-white dark:border-sui-black border-b-sui-gray-50 dark:border-b-sui-gray-80">
      {tabs.map(({ label, indexName, count }) => (
        <div className="relative group inline-block">
          <button
            key={indexName}
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

function IndexStatsCollector({
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

function RefinementSection() {
  const { hits } = useHits();
  if (hits.length === 0) return null;
  return (
    <div className="bg-sui-gray-50 dark:bg-sui-gray-80 p-4 sticky top-20 z-10">
      <h2 className="text-lg">Refine results</h2>
      <RefinementList attribute="source" />
    </div>
  );
}

function ConditionalPagination() {
  const { hits } = useHits();
  if (hits.length === 0) return null;
  return <Pagination />;
}

function TabbedIndex({ indexName }) {
  const { hits } = useHits();
  return (
    <Index indexName={indexName}>
      <div className="grid grid-cols-12 gap-4">
        {hits.length > 0 && (
          <div className="col-span-12 md:col-span-4 xl:col-span-3">
            <RefinementSection />
          </div>
        )}
        <div className="col-span-12 md:col-span-8 xl:col-span-9">
          <CustomHitsContent name={indexName} />
        </div>
        <div className="col-span-12">
          <ConditionalPagination />
        </div>
      </div>
    </Index>
  );
}

export default function Search() {
  const searchClient = algoliasearch(
    "M9JD2UP87M",
    "826134b026a63bb35692f08f1dc85d1c",
  );

  const queryParam = getQueryParam("q");
  const [activeTab, setActiveTab] = React.useState("sui_docs");
  const [tabCounts, setTabCounts] = React.useState<Record<string, number>>({
    sui_docs: 0,
  });
  const [query, setQuery] = React.useState(queryParam);

  const tabs = [
    { label: "Sui", indexName: "sui_docs" },
    { label: "SuiNS", indexName: "suins_docs" },
    { label: "The Move Book", indexName: "move_book" },
    { label: "SDKs", indexName: "sui_sdks" },
    { label: "Walrus", indexName: "walrus_docs" },
  ];

  const handleVisibility = React.useCallback(
    (indexName: string, nbHits: number) => {
      setTabCounts((prev) => ({ ...prev, [indexName]: nbHits }));
    },
    [],
  );

  return (
    <InstantSearch
      searchClient={searchClient}
      indexName="sui_docs"
      future={{ preserveSharedStateOnUnmount: true }}
      initialUiState={{
        sui_docs: { query: queryParam },
        suins_docs: { query: queryParam },
        move_book: { query: queryParam },
        sui_sdks: { query: queryParam },
        walrus_docs: { query: queryParam },
      }}
    >
      {/* Preload tab visibility */}
      {tabs.map((tab) => (
        <Index indexName={tab.indexName} key={`stat-${tab.indexName}`}>
          <IndexStatsCollector
            indexName={tab.indexName}
            onUpdate={handleVisibility}
          />
        </Index>
      ))}

      <div className="grid grid-cols-12 gap-4 sui-search">
        <div className="col-span-12">
          <ControlledSearchBox
            placeholder={`Search`}
            query={query}
            onChange={setQuery}
          />
        </div>
        <div className="col-span-12">
          <TabbedResults
            activeTab={activeTab}
            onChange={setActiveTab}
            tabs={tabs.map((tab) => ({
              ...tab,
              count: tabCounts[tab.indexName] || 0,
            }))}
          />
        </div>
        <div className="col-span-12">
          {tabs.map((tab) => (
            <div
              key={tab.indexName}
              className={`flex ${activeTab === tab.indexName ? "block" : "hidden"}`}
            >
              <TabbedIndex indexName={tab.indexName} query={query} />
            </div>
          ))}
        </div>
      </div>
    </InstantSearch>
  );
}
