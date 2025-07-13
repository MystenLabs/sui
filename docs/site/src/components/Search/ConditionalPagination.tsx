// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React from "react";
import { useHits, usePagination } from "react-instantsearch";

function CustomPagination() {
  const { currentRefinement, nbPages, refine, pages } = usePagination();

  if (nbPages <= 1) return null;

  return (
    <ul className="flex gap-2 mt-4 justify-center list-none">
      {nbPages > 2 && (
        <li
          onClick={() => refine(0)}
          className={`px-3 py-1 border cursor-pointer bg-sui-gray-50 dark:bg-sui-gray-80 ${
            currentRefinement === 0
              ? "opacity-50 cursor-not-allowed"
              : "text-blue-600 border-blue-600"
          }`}
        >
          &laquo;
        </li>
      )}
      {nbPages > 1 && (
        <li
          onClick={() => currentRefinement > 0 && refine(currentRefinement - 1)}
          className={`px-3 py-1 border cursor-pointer bg-sui-gray-50 dark:bg-sui-gray-80 ${
            currentRefinement === 0
              ? "opacity-50 cursor-not-allowed"
              : "text-blue-600 border-blue-600"
          }`}
        >
          &lsaquo;
        </li>
      )}
      {pages.map((page) => {
        const isActive = currentRefinement === page;
        return (
          <li
            key={page}
            onClick={() => refine(page)}
            className={`px-3 py-1 border cursor-pointer bg-sui-gray-50 dark:bg-sui-gray-80 ${
              isActive
                ? "bg-blue-600 text-white"
                : "text-blue-600 border-blue-600"
            }`}
          >
            {page + 1}
          </li>
        );
      })}
      {nbPages > 1 && (
        <li
          onClick={() =>
            currentRefinement < nbPages - 1 && refine(currentRefinement + 1)
          }
          className={`px-3 py-1 border cursor-pointer bg-sui-gray-50 dark:bg-sui-gray-80 ${
            currentRefinement === nbPages - 1
              ? "opacity-50 cursor-not-allowed"
              : "text-blue-600 border-blue-600"
          }`}
        >
          &rsaquo;
        </li>
      )}
      {nbPages > 2 && (
        <li
          onClick={() => refine(nbPages - 1)}
          className={`px-3 py-1 border cursor-pointer bg-sui-gray-50 dark:bg-sui-gray-80 ${
            currentRefinement === nbPages - 1
              ? "opacity-50 cursor-not-allowed"
              : "text-blue-600 border-blue-600"
          }`}
        >
          &raquo;
        </li>
      )}
    </ul>
  );
}

export default function ConditionalPagination() {
  const { hits } = useHits();
  if (hits.length === 0) return null;
  return <CustomPagination />;
}
