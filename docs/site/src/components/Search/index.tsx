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
import { useHistory } from "@docusaurus/router";

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

function CustomHitsContent({ name }) {
  const { hits: items } = useHits();
  const history = useHistory();
  const currentHost = typeof window !== "undefined" ? window.location.host : "";

  let siteToVisit = "Try your search again with different keywords";
  if (name === "sui_docs") {
    siteToVisit = `${siteToVisit}. If you are unable to find the information you need, try one of the official Sui support channels: <a href="https://github.com/MystenLabs/sui/issues/new/choose" target="_blank">GitHub</a>, <a href="https://discord.gg/Sui" target="_blank">Discord</a>, or <a href="https://t.me/SuiTokenNetwork" target="_blank">Telegram</a>.`;
  } else if (name === "suins_docs") {
    siteToVisit = `${siteToVisit} or visit the official <a href="https://docs.suins.io" target="_blank">SuiNS doc</a> site.`;
  } else if (name === "move_book") {
    siteToVisit = `${siteToVisit} or visit <a href="https://move-book.com/" target="_blank">The Move Book</a> dedicated site.`;
  } else if (name === "dapp_kit") {
    siteToVisit = `${siteToVisit} or visit the official <a href="https://sdk.mystenlabs.com/dapp-kit" target="_blank">dApp Kit</a> site.`;
  } else {
    siteToVisit = `${siteToVisit}.`;
  }

  if (items.length === 0) {
    return (
      <>
        <p>No results found.</p>
        <p
          dangerouslySetInnerHTML={{
            __html: `${siteToVisit}`,
          }}
        />
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
          <div
            className="mb-8 pb-8 border-2 border-solid border-transparent border-b-sui-gray-50 dark:border-b-sui-gray-80"
            key={index}
          >
            <div className="text-2xl text-sui-gray-80 text-bold mb-2">
              {group[0].hierarchy?.lvl1 || "[no title]"}
            </div>
            <div className="ml-4">
              {group.map((hit, i) => {
                const level = hit.type;
                let sectionTitle = hit.lvl0;
                if (level === "content") {
                  sectionTitle = getDeepestHierarchyLabel(hit.hierarchy);
                } else {
                  sectionTitle = hit.hierarchy?.[level] || level;
                }

                const hitHost = new URL(hit.url).host;
                const isInternal = hitHost === currentHost;

                return (
                  <div key={i} className="mb-2">
                    {isInternal ? (
                      <button
                        onClick={() => history.push(new URL(hit.url).pathname)}
                        className="text-lg text-blue-600 underline text-left"
                      >
                        {sectionTitle}
                      </button>
                    ) : (
                      <a
                        href={hit.url}
                        target="_blank"
                        rel="noopener noreferrer"
                        className="text-lg text-blue-600 underline"
                      >
                        {sectionTitle}
                      </a>
                    )}
                    <p>{hit.content ? truncateAtWord(hit.content) : ""}</p>
                  </div>
                );
              })}
            </div>
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

  const tabs = [
    { label: "Sui", indexName: "sui_docs" },
    { label: "SuiNS", indexName: "suins_docs" },
    { label: "The Move Book and Reference", indexName: "move_book" },
    { label: "dApp Kit", indexName: "dapp_kit" },
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
          <SearchBox placeholder="Start typing to begin your search..." />
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
              <TabbedIndex indexName={tab.indexName} />
            </div>
          ))}
        </div>
      </div>
    </InstantSearch>
  );
}
