// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React from "react";
import { liteClient as algoliasearch } from "algoliasearch/lite";
import {
  InstantSearch,
  SearchBox,
  RefinementList,
  useInfiniteHits,
  Index,
  Stats,
  Pagination,
  useStats,
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

function CustomHits({ label }) {
  const { items } = useInfiniteHits();
  const [expanded, setExpanded] = React.useState<Record<string, boolean>>({});
  const { nbHits } = useStats();

  const grouped = items.reduce(
    (acc, hit) => {
      const key = hit.url_without_anchor;
      if (!acc[key]) acc[key] = [];
      acc[key].push(hit);
      return acc;
    },
    {} as Record<string, typeof items>,
  );

  const toggle = (key: string) => {
    setExpanded((prev) => ({ ...prev, [key]: !prev[key] }));
  };

  //setTotal(Object.keys(grouped).length);

  return (
    <>
      {nbHits > 0 && (
        <div>
          <h2>
            {label} {nbHits}
          </h2>
          <RefinementList attribute="source" />
          {Object.entries(grouped).map(([key, group], index) => {
            const isOpen = expanded[key] ?? false;
            return (
              <div
                className="border border-solid p-4 mb-4 rounded-lg"
                key={index}
              >
                <button
                  className="font-bold text-left w-full"
                  onClick={() => toggle(key)}
                >
                  {group[0].hierarchy?.lvl1 || "[no title]"}
                </button>
                {isOpen &&
                  group.map((hit, i) => {
                    const level = hit.type;
                    let sectionTitle = hit.lvl0;
                    if (level === "content") {
                      sectionTitle = getDeepestHierarchyLabel(hit.hierarchy);
                    } else {
                      sectionTitle = hit.hierarchy?.[level] || level;
                    }
                    return (
                      <div key={i} className="mb-2">
                        <a
                          href={hit.url}
                          className="text-sm text-blue-600 underline"
                        >
                          {sectionTitle}
                        </a>
                        <p>{hit.content ? truncateAtWord(hit.content) : ""}</p>
                      </div>
                    );
                  })}
              </div>
            );
          })}
        </div>
      )}
    </>
  );
}

function getQueryParam(key) {
  const params = new URLSearchParams(
    typeof window !== "undefined" ? window.location.search : "",
  );
  return params.get(key) || "";
}

export default function Search() {
  const searchClient = algoliasearch(
    "M9JD2UP87M",
    "826134b026a63bb35692f08f1dc85d1c",
  );

  const queryParam = getQueryParam("q");

  return (
    <InstantSearch
      searchClient={searchClient}
      indexName="Sui Docs"
      future={{ preserveSharedStateOnUnmount: true }}
      initialUiState={{
        sui_docs: {
          query: queryParam,
        },
        suins_docs: {
          query: queryParam,
        },
      }}
    >
      <div className="grid grid-cols-12 gap-4 sui-search">
        <div className="col-span-12">
          <SearchBox />
        </div>
        <div className="col-span-3 border border-solid rounded-lg w-full h-auto self-start">
          <h1 className="text-lg pl-2 bg-sui-blue-dark rounded-t-lg text-white">
            Sources
          </h1>
        </div>
        <div className="col-span-9">
          <Index indexName="sui_docs">
            <CustomHits label="Sui docs" />
            <Pagination />
          </Index>
          <Index indexName="suins_docs">
            <CustomHits label="SuiNS docs" />
          </Index>
          <Index indexName="move_book">
            <CustomHits label="The Move Book and Reference" />
          </Index>
          <Index indexName="dapp_kit">
            <CustomHits label="SDK docs" />
          </Index>
        </div>
      </div>
    </InstantSearch>
  );
}
