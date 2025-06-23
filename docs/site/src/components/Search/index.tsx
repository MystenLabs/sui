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
  return (
    <div className="mb-4 flex justify-center">
      {tabs.map(({ label, indexName, count }) => (
        <button
          key={indexName}
          className="mx-4"
          onClick={() => onChange(indexName)}
          disabled={activeTab === indexName}
        >
          {label} | {count}
        </button>
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
  const { nbHits } = useStats();

  if (nbHits === 0) return null;

  return (
    <div className="py-4 border border-solid rounded-lg mb-4">
      <h2 className="pl-4 text-lg">Filter by category</h2>
      <RefinementList attribute="source" />
    </div>
  );
}

function TabbedIndex({ indexName }) {
  const { nbHits } = useStats();
  console.log(nbHits);
  if (nbHits === 0) return null;
  return (
    <Index indexName={indexName}>
      <RefinementSection />
      <CustomHitsContent />
      <Pagination />
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
