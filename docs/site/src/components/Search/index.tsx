// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React from "react";
import { liteClient as algoliasearch } from "algoliasearch/lite";
import {
  InstantSearch,
  SearchBox,
  RefinementList,
  useHits,
  useStats,
  Index,
  Pagination,
} from "react-instantsearch";

const { decode } = require("he");

function truncateAtWord(text, maxChars = 250) {
  if (text.length <= maxChars) return text;
  const decoded = decode(text);
  const truncated = decoded.slice(0, maxChars);
  return truncated.slice(0, truncated.lastIndexOf(" ")) + "…";
}

function getDeepestHierarchyLabel(hierarchy) {
  const levels = ["lvl0", "lvl1", "lvl2", "lvl3", "lvl4", "lvl5", "lvl6"];
  let lastValue = null;

  for (const lvl of levels) {
    const value = hierarchy[lvl];
    if (value == null) {
      break;
    }
    lastValue = value;
  }

  return lastValue || hierarchy.lvl6 || "";
}

function CustomHitsContent() {
  const { hits: items } = useHits();

  if (items.length === 0) {
    return (
      <>
        <p>No results found.</p>
        <p>
          Try your search again with different keywords or visit such and such
          site.
        </p>
      </>
    );
  }

  const grouped = items.reduce(
    (acc, hit) => {
      const key = hit.url_without_anchor;
      if (!acc[key]) acc[key] = [];
      acc[key].push(hit);
      return acc;
    },
    {} as Record<string, typeof items>,
  );

  return (
    <>
      {Object.entries(grouped).map(([key, group], index) => {
        return (
          <div className="border border-solid p-4 mb-4 rounded-lg" key={index}>
            <div className="font-bold text-left w-full">
              {group[0].hierarchy?.lvl1 || "[no title]"}
            </div>
            {group.map((hit, i) => {
              const level = hit.type;
              let sectionTitle = hit.lvl0;
              if (level === "content") {
                sectionTitle = getDeepestHierarchyLabel(hit.hierarchy);
              } else {
                sectionTitle = hit.hierarchy?.[level] || level;
              }
              return (
                <div key={i} className="mb-2">
                  <a href={hit.url} className="text-sm text-blue-600 underline">
                    {sectionTitle}
                  </a>
                  <p>{hit.content ? truncateAtWord(hit.content) : ""}</p>
                </div>
              );
            })}
          </div>
        );
      })}
    </>
  );
}

function getQueryParam(key) {
  const params = new URLSearchParams(
    typeof window !== "undefined" ? window.location.search : "",
  );
  return params.get(key) || "";
}

function TabbedResults({ activeTab, onChange, tabs }) {
  return (
    <div className="mb-4 flex justify-start">
      {tabs.map(({ label, indexName, count }) => (
        <button
          key={indexName}
          className="mr-4 text-lg border-none bg-white"
          onClick={() => onChange(indexName)}
          active={activeTab === indexName}
        >
          {label} | <span className="text-sm">{count} hits</span>
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

  const tabs = [
    { label: "Sui docs", indexName: "sui_docs" },
    { label: "SuiNS docs", indexName: "suins_docs" },
    { label: "The Move Book and Reference", indexName: "move_book" },
    { label: "SDK docs", indexName: "dapp_kit" },
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
      indexName="Sui Docs"
      future={{ preserveSharedStateOnUnmount: true }}
      initialUiState={{
        sui_docs: { query: queryParam },
        suins_docs: { query: queryParam },
        move_book: { query: queryParam },
        dapp_kit: { query: queryParam },
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
          <SearchBox />
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
              style={{
                display: activeTab === tab.indexName ? "block" : "none",
              }}
            >
              <TabbedIndex indexName={tab.indexName} />
            </div>
          ))}
        </div>
      </div>
    </InstantSearch>
  );
}
