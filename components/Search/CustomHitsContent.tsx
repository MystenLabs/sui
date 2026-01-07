
import React from "react";
import { useHits } from "react-instantsearch";
import { useHistory } from "@docusaurus/router";
import { truncateAtWord, getDeepestHierarchyLabel } from "./utils";

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
        return (
          <div
            className="p-6 pb-[40px] mb-6 bg-sui-gray-35 rounded-[20px]"
            key={index}
          >
            <div className="text-xl text-sui-gray-3s font-semibold mb-4">
              {group[0].hierarchy?.lvl1 ||
                group[0].hierarchy?.lvl0 ||
                "[no title]"}
            </div>
            <div className="">
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
                        className="text-base text-blue-600 hover:text-sui-blue underline text-left bg-transparent border-0 pl-0 cursor-pointer font-[Inter]"
                      >
                        {sectionTitle}
                      </button>
                    ) : (
                      <a
                        href={hit.url}
                        target="_blank"
                        rel="noopener noreferrer"
                        className="text-base text-blue-600 underline pb-2"
                      >
                        {sectionTitle}
                      </a>
                    )}
                    <p
                      className="font-normal text-base text-sui-gray-5s"
                      dangerouslySetInnerHTML={{
                        __html: hit.content
                          ? truncateAtWord(hit._highlightResult.content.value)
                          : "",
                      }}
                    />
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
