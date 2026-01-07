
import React from "react";
import { Index } from "react-instantsearch";
import VirtualSearchBox from "./VirtualSearchBox";
import RefinementSection from "./RefinementSection";
import ConditionalPagination from "./ConditionalPagination";
import CustomHitsContent from "./CustomHitsContent";

export default function TabbedIndex({
  indexName,
  query,
}: {
  indexName: string;
  query: string;
}) {
  return (
    <Index indexName={indexName}>
      <VirtualSearchBox query={query} />
      <div className="grid grid-cols-12 gap-4">
        <RefinementSection />
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
