// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React, { useState, useEffect } from "react";
import { liteClient as algoliasearch } from "algoliasearch/lite";
import {
  InstantSearch,
  useInfiniteHits,
  useInstantSearch,
  useSearchBox,
  Index,
} from "react-instantsearch";
import {
  truncateAtWord,
  getDeepestHierarchyLabel,
  getHierarchyBreadcrumbs,
  cleanTooltipText,
} from "./utils";

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
  const crumbs = getHierarchyBreadcrumbs(hit.hierarchy);
  const title = crumbs.length > 0 ? crumbs[crumbs.length - 1] : cleanTooltipText(hit.hierarchy?.lvl0 || "Untitled");
  const breadcrumb = crumbs.length > 1 ? crumbs.slice(0, -1) : [];

  return (
    <a
      href={hit.url}
      className="modal-result block px-4 py-3 -mx-2 rounded-lg no-underline hover:bg-sui-gray-40 dark:hover:bg-sui-gray-80 transition-colors"
    >
      {breadcrumb.length > 0 && (
        <div className="text-xs text-gray-500 dark:text-sui-gray-55 mb-1 truncate">
          {breadcrumb.join(" > ")}
        </div>
      )}
      <div className="text-sm font-medium text-gray-900 dark:text-white">
        {title}
      </div>
      {hit.content && (
        <p
          className="text-xs text-gray-600 dark:text-sui-gray-45 mt-1 mb-0 line-clamp-2"
          dangerouslySetInnerHTML={{
            __html: truncateAtWord(hit._highlightResult.content.value, 120),
          }}
        />
      )}
    </a>
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
    return (
      <p className="text-sm text-sui-gray-5s dark:text-sui-gray-50">
        No results in {label}
      </p>
    );
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
  }, [results?.nbHits, indexName, onUpdate, results]);
  return null;
}

/** Syncs the external query state into Algolia's InstantSearch. */
function QuerySync({ query }: { query: string }) {
  const { refine } = useSearchBox();
  useEffect(() => { refine(query); }, [query, refine]);
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
  const searchBoxRef = React.useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (isOpen) {
      document.body.style.overflow = "hidden";
      setTimeout(() => {
        searchBoxRef.current?.focus();
      }, 300);
    } else {
      document.body.style.overflow = "";
    }
    return () => {
      document.body.style.overflow = "";
    };
  }, [isOpen]);

  useEffect(() => {
    if (!isOpen) return;
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [isOpen, onClose]);

  const activeMeta = {
    sui_docs: null,
    suins_docs: { label: "SuiNS Docs", url: "https://docs.suins.io" },
    move_book: {
      label: "The Move Book",
      url: "https://move-book.com/",
    },
    sui_sdks: { label: "SDK Docs", url: "https://sdk.mystenlabs.com" },
    walrus_docs: { label: "Walrus Docs", url: "https://docs.wal.app" },
  }[activeIndex];

  if (!isOpen) return null;
  const modalBg = "bg-white dark:bg-[#1a1a2e]";

  return (
    <div
      className="fixed inset-0 bg-black/50 z-50 flex justify-center items-start pt-[8vh]"
      onClick={(e) => { if (e.target === e.currentTarget) onClose(); }}
    >
      <div className={`${modalBg} w-full max-w-2xl rounded-2xl shadow-2xl max-h-[min(640px,82vh)] flex flex-col overflow-hidden mx-4`}>
        {/* Header with input */}
        <div className={`${modalBg} shrink-0 px-5 pt-4 pb-0`}>
          <div className="flex items-center justify-between mb-3">
            <span className="text-xs font-medium text-gray-400 dark:text-gray-500 uppercase tracking-wider">Search or ask</span>
            <button
              onClick={onClose}
              className="bg-transparent border border-solid border-gray-200 dark:border-gray-700 rounded px-1.5 py-0.5 text-[10px] text-gray-400 dark:text-gray-500 cursor-pointer hover:text-gray-600"
            >
              ESC
            </button>
          </div>

          {/* Ask Sui AI — primary action */}
          <button
            type="button"
            className="w-full flex items-center gap-3 px-4 py-3.5 mb-3 rounded-xl bg-gradient-to-r from-[#298DFF] to-[#1B6FD1] text-white border-none cursor-pointer transition-all hover:shadow-lg hover:shadow-blue-500/20 active:scale-[0.99]"
            onClick={() => {
              onClose();
              if (typeof window !== "undefined" && window.Kapa) {
                setTimeout(() => window.Kapa.open(query || undefined), 100);
              }
            }}
          >
            <div className="w-9 h-9 rounded-lg bg-white/20 flex items-center justify-center shrink-0">
              <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                <path d="M15 4V2" /><path d="M15 16v-2" /><path d="M8 9h2" /><path d="M20 9h2" />
                <path d="M17.8 11.8L19 13" /><path d="M15 9h.01" /><path d="M17.8 6.2L19 5" />
                <path d="M11 6.2L9.7 5" /><path d="M11 11.8L9.7 13" />
                <path d="M8 15h8a2 2 0 0 0 2-2V7a2 2 0 0 0-2-2H8a2 2 0 0 0-2 2v6a2 2 0 0 0 2 2z" />
                <path d="M9 18h6" /><path d="M10 22h4" /><path d="M10 18v4" /><path d="M14 18v4" />
              </svg>
            </div>
            <div className="text-left flex-1">
              <div className="font-semibold text-[15px] leading-tight">Ask Sui AI</div>
              <div className="text-xs text-white/70 mt-0.5">Get instant answers from the Sui knowledge base</div>
            </div>
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="text-white/50">
              <polyline points="9,18 15,12 9,6" />
            </svg>
          </button>

          {/* Search input */}
          <div className="flex items-center gap-2 px-3 py-2 rounded-lg border border-solid border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-white/5 mb-3">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="text-gray-400 shrink-0">
              <circle cx="11" cy="11" r="8" /><line x1="21" y1="21" x2="16.65" y2="16.65" />
            </svg>
            <input
              ref={searchBoxRef}
              type="search"
              className="w-full bg-transparent border-none outline-none text-sm text-gray-900 dark:text-gray-200 placeholder-gray-400 dark:placeholder-gray-500"
              placeholder="Search documentation..."
              value={query}
              onChange={(e) => {
                setQuery(e.currentTarget.value);
              }}
            />
          </div>
        </div>

        {/* Tabs + results */}
        <div ref={scrollContainerRef} className="flex-1 overflow-y-auto">
          <InstantSearch searchClient={searchClient} indexName={activeIndex}>
            {/* Sync query to Algolia */}
            <QuerySync query={query} />
            <div className={`${modalBg} sticky top-0 z-10 px-5`}>
              <div className="flex items-center gap-1 border-b border-solid border-gray-200 dark:border-gray-700 mb-0">
                {indices.map(({ label, indexName }) => (
                  <button
                    key={indexName}
                    className={`px-3 py-2 text-xs font-medium border-none bg-transparent cursor-pointer transition-colors ${
                      activeIndex === indexName
                        ? "text-[#298DFF] border-b-2 border-solid border-[#298DFF] -mb-px"
                        : "text-gray-500 dark:text-gray-400 hover:text-gray-700 dark:hover:text-gray-300"
                    }`}
                    onClick={() => setActiveIndex(indexName)}
                  >
                    {label}
                    {(tabCounts[indexName] || 0) > 0 && (
                      <span className={`ml-1.5 text-[10px] rounded-full px-1.5 py-0.5 ${
                        activeIndex === indexName
                          ? "bg-blue-50 dark:bg-blue-900/30 text-[#298DFF]"
                          : "bg-gray-100 dark:bg-gray-800 text-gray-400"
                      }`}>
                        {tabCounts[indexName]}
                      </span>
                    )}
                  </button>
                ))}
              </div>
            </div>
            <div className="px-5 py-3">
              {query.length < 3 ? (
                <p className="text-sm text-gray-400 dark:text-gray-500 text-center py-6">
                  Type at least 3 characters to search
                </p>
              ) : (
                indices.map((index) => (
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
                ))
              )}
            </div>
          </InstantSearch>
        </div>

        {/* Footer */}
        <div className={`h-10 px-5 ${modalBg} flex items-center justify-between text-[11px] border-t border-solid border-gray-200 dark:border-gray-700 shrink-0`}>
          <a
            href={`/search?q=${encodeURIComponent(query)}`}
            className="text-gray-400 dark:text-gray-500 hover:text-[#298DFF] no-underline"
          >
            View all results →
          </a>
          {activeMeta && (
            <a
              href={activeMeta.url}
              target="_blank"
              rel="noopener noreferrer"
              className="text-gray-400 dark:text-gray-500 hover:text-[#298DFF] no-underline"
            >
              {activeMeta.label} →
            </a>
          )}
        </div>
      </div>
    </div>
  );
}
