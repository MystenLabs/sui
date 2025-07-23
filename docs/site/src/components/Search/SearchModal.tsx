// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React, { useState, useEffect } from "react";
import { liteClient as algoliasearch } from "algoliasearch/lite";
import {
  InstantSearch,
  useInfiniteHits,
  useInstantSearch,
  Index,
} from "react-instantsearch";
import { truncateAtWord, getDeepestHierarchyLabel } from "./utils";
import ControlledSearchBox from "./ControlledSearchBox";
import TabbedResults from "./TabbedResults";

const baseSearchClient = algoliasearch(
  "M9JD2UP87M",
  "826134b026a63bb35692f08f1dc85d1c",
);

const searchClient = {
  ...baseSearchClient,
  search(requests: any[]) {
    const hasValidQuery = requests.some(
      (req) => req.params?.query?.length >= 3,
    );
    if (!hasValidQuery) {
      return Promise.resolve({
        results: requests.map(() => ({
          hits: [],
          nbHits: 0,
          processingTimeMS: 0,
        })),
      });
    }
    return baseSearchClient.search(requests);
  },
};

const indices = [
  { label: "Sui", indexName: "sui_docs" },
  { label: "SuiNS", indexName: "suins_docs" },
  { label: "The Move Book", indexName: "move_book" },
  { label: "SDKs", indexName: "sui_sdks" },
  { label: "Walrus", indexName: "walrus_docs" },
];

function HitItem({ hit }: { hit: any }) {
  const level = hit.type;
  let sectionTitle = hit.lvl0;
  if (level === "content") {
    sectionTitle = getDeepestHierarchyLabel(hit.hierarchy);
  } else {
    sectionTitle = hit.hierarchy?.[level] || level;
  }
  return (
    <div className="border-b">
      <a href={hit.url} className="text-blue-600 font-medium">
        {hit.title}
      </a>
      <a
        href={hit.url}
        target="_blank"
        rel="noopener noreferrer"
        className="text-base text-blue-600 underline pb-2"
      >
        {sectionTitle}
      </a>
      <p
        className="text-sm text-gray-600"
        dangerouslySetInnerHTML={{
          __html: hit.content
            ? truncateAtWord(hit._highlightResult.content.value, 100)
            : "",
        }}
      ></p>
    </div>
  );
}

function HitsList({
  scrollContainerRef,
}: {
  scrollContainerRef: React.RefObject<HTMLDivElement>;
}) {
  const { hits, isLastPage, showMore } = useInfiniteHits();

  useEffect(() => {
    const el = scrollContainerRef.current;
    if (!el) return;

    const handleScroll = () => {
      const atBottom = el.scrollTop + el.clientHeight >= el.scrollHeight - 1;
      if (atBottom && !isLastPage) {
        showMore();
      }
    };

    el.addEventListener("scroll", handleScroll);
    return () => el.removeEventListener("scroll", handleScroll);
  }, [isLastPage, showMore, scrollContainerRef]);

  return (
    <div>
      {hits.map((hit) => (
        <HitItem key={hit.objectID} hit={hit} />
      ))}
    </div>
  );
}

function EmptyState({ label }: { label: string }) {
  const { results } = useInstantSearch();
  if (results?.hits?.length === 0) {
    return <p className="text-sm text-gray-500">No results in {label}</p>;
  }
  return null;
}

function ResultsUpdater({
  indexName,
  onUpdate,
}: {
  indexName: string;
  onUpdate: (index: string, count: number) => void;
}) {
  const { results } = useInstantSearch();
  const previousHitsRef = React.useRef<number | null>(null);
  useEffect(() => {
    if (results && results.nbHits !== previousHitsRef.current) {
      previousHitsRef.current = results.nbHits;
      onUpdate(indexName, results.nbHits);
    }
  }, [results?.nbHits, indexName, onUpdate]);
  return null;
}

export default function MultiIndexSearchModal({
  isOpen,
  onClose,
}: {
  isOpen: boolean;
  onClose: () => void;
}) {
  const [activeIndex, setActiveIndex] = useState(indices[0].indexName);
  const [tabCounts, setTabCounts] = React.useState<Record<string, number>>({
    sui_docs: 0,
  });
  const [query, setQuery] = React.useState("");
  const scrollContainerRef = React.useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (isOpen) {
      document.body.style.overflow = "hidden";
    } else {
      document.body.style.overflow = "";
    }
    return () => {
      document.body.style.overflow = "";
    };
  }, [isOpen]);

  const activeMeta = {
    sui_docs: null,
    suins_docs: { label: "SuiNS Docs", url: "https://docs.suins.io" },
    move_book: {
      label: "The Move Book",
      url: "https://move-book.com/",
    },
    sui_sdks: { label: "SDK Docs", url: "https://sdk.mystenlabs.com" },
    walrus_docs: { label: "Walrus Docs", url: "https://docs.walrus.app" },
  }[activeIndex];

  if (!isOpen) return null;
  return (
    <div className="fixed inset-0 bg-black/50 z-50 flex justify-center p-4">
      <div className="bg-white w-full max-w-3xl px-6 rounded-lg shadow-md h-3xl flex flex-col">
        <div ref={scrollContainerRef} className="flex-1 overflow-y-auto">
          <InstantSearch searchClient={searchClient} indexName={activeIndex}>
            <div className="bg-white rounded-t sticky top-0 z-10">
              <div className="bg-white h-8 flex justify-end">
                <button
                  onClick={onClose}
                  className="bg-transparent border-none outline-none text-sm underline cursor-pointer"
                >
                  Close
                </button>
              </div>
              <ControlledSearchBox
                placeholder={`Search`}
                query={query}
                onChange={setQuery}
              />
              {query.length < 3 && (
                <p className="text-sm text-gray-500 mb-2 -mt-6">
                  Three characters starts search...
                </p>
              )}
              <TabbedResults
                activeTab={activeIndex}
                onChange={setActiveIndex}
                showTooltips={false}
                tabs={indices.map((tab) => ({
                  ...tab,
                  count: tabCounts[tab.indexName] || 0,
                }))}
              />
            </div>
            {indices.map((index) => (
              <Index indexName={index.indexName} key={index.indexName}>
                <ResultsUpdater
                  indexName={index.indexName}
                  onUpdate={(indexName, count) =>
                    setTabCounts((prev) => ({ ...prev, [indexName]: count }))
                  }
                />
                {index.indexName === activeIndex && (
                  <>
                    <HitsList scrollContainerRef={scrollContainerRef} />
                    <EmptyState label={index.label} />
                  </>
                )}
              </Index>
            ))}
          </InstantSearch>
        </div>
        <div className="h-14 bg-white flex items-center justify-between text-sm border-t border-solid border-sui-gray-50 border-b-transparent border-l-transparent border-r-transparent">
          <a
            href={`/search?q=${encodeURIComponent(query)}`}
            className="text-blue-600 underline"
          >
            Go to full search page
          </a>
          {activeMeta && (
            <a
              href={activeMeta.url}
              target="_blank"
              rel="noopener noreferrer"
              className="text-blue-600 underline"
            >
              Visit {activeMeta.label} →
            </a>
          )}
        </div>
      </div>
    </div>
  );
}
