// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React from "react";
import { useHits } from "react-instantsearch";
import { useHistory } from "@docusaurus/router";
import {
  truncateAtWord,
  getDeepestHierarchyLabel,
  getHierarchyBreadcrumbs,
  cleanTooltipText,
} from "./utils";

export default function CustomHitsContent({ name }) {
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
  } else if (name === "sui_sdks") {
    siteToVisit = `${siteToVisit} or visit the official <a href="https://sdk.mystenlabs.com" target="_blank">Sui SDKs</a> site.`;
  } else if (name === "walrus_sdks") {
    siteToVisit = `${siteToVisit} or visit the official <a href="https://docs.wal.app/" target="_blank">Walrus Docs</a> site.`;
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
        const pageCrumbs = getHierarchyBreadcrumbs(group[0].hierarchy);
        const pageTitle =
          pageCrumbs.length > 0
            ? pageCrumbs[Math.min(1, pageCrumbs.length - 1)]
            : "[no title]";

        return (
          <div
            className="p-6 pb-6 mb-6 bg-sui-gray-35 dark:bg-sui-gray-85 rounded-2xl"
            key={index}
          >
            <div className="text-lg font-semibold mb-1 text-gray-900 dark:text-white">
              {pageTitle}
            </div>
            {pageCrumbs.length > 0 && (
              <div className="text-xs text-gray-500 dark:text-sui-gray-50 mb-4">
                {pageCrumbs.join(" > ")}
              </div>
            )}
            <div className="space-y-3">
              {group.map((hit, i) => {
                const hitCrumbs = getHierarchyBreadcrumbs(hit.hierarchy);
                const sectionTitle =
                  hitCrumbs.length > 0
                    ? hitCrumbs[hitCrumbs.length - 1]
                    : cleanTooltipText(
                        getDeepestHierarchyLabel(hit.hierarchy),
                      );

                const hitHost = new URL(hit.url).host;
                const isInternal = hitHost === currentHost;

                return (
                  <div key={i} className="py-1">
                    {isInternal ? (
                      <button
                        onClick={() => history.push(new URL(hit.url).pathname)}
                        className="text-sm text-blue-700 dark:text-sui-blue-light hover:text-sui-blue-dark dark:hover:text-white font-medium underline text-left bg-transparent border-0 pl-0 cursor-pointer"
                      >
                        {sectionTitle}
                      </button>
                    ) : (
                      <a
                        href={hit.url}
                        target="_blank"
                        rel="noopener noreferrer"
                        className="text-sm text-blue-700 dark:text-sui-blue-light hover:text-sui-blue-dark dark:hover:text-white font-medium underline"
                      >
                        {sectionTitle}
                      </a>
                    )}
                    {hit.content && (
                      <p
                        className="font-normal text-sm text-gray-600 dark:text-sui-gray-45 mt-1"
                        dangerouslySetInnerHTML={{
                          __html: truncateAtWord(
                            hit._highlightResult.content.value,
                          ),
                        }}
                      />
                    )}
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
