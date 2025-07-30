// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import React from "react";
import { useHits, usePagination } from "react-instantsearch";

const step = (
  <svg
    width="12"
    height="18"
    viewBox="0 0 12 12"
    fill="none"
    xmlns="http://www.w3.org/2000/svg"
  >
    <path
      d="M2.47885 0.806646L10.3905 5.1221C11.0854 5.50112 11.0854 6.49888 10.3905 6.8779L2.47885 11.1934C1.81248 11.5568 1 11.0745 1 10.3155V1.68454C1 0.925483 1.81248 0.443169 2.47885 0.806646Z"
      stroke="#A0B6C3"
    />
  </svg>
);

const jump = (
  <svg
    width="20"
    height="18"
    viewBox="0 0 20 12"
    fill="none"
    xmlns="http://www.w3.org/2000/svg"
  >
    <path
      d="M2.47885 0.806646L10.3905 5.1221C11.0854 5.50112 11.0854 6.49888 10.3905 6.8779L2.47885 11.1934C1.81248 11.5568 1 11.0745 1 10.3155V1.68454C1 0.925483 1.81248 0.443169 2.47885 0.806646Z"
      fill="white"
      fill-opacity="0.8"
      stroke="#A0B6C3"
    />
    <path
      d="M10.4789 0.806646L18.3905 5.1221C19.0854 5.50112 19.0854 6.49888 18.3905 6.8779L10.4789 11.1934C9.81248 11.5568 9 11.0745 9 10.3155V1.68454C9 0.925483 9.81248 0.443169 10.4789 0.806646Z"
      fill="white"
      fill-opacity="0.8"
      stroke="#A0B6C3"
    />
  </svg>
);

const pageItemStyle =
  "px-3 py-[9px] border border-solid border-sui-gray-50 hover:border-sui-blue-dark cursor-pointer rounded-md text-sm text-sui-steel-dark dark:text-sui-blue dark:hover:border-sui-blue";
const disabledItemStyle =
  "px-3 py-[9px] opacity-50 cursor-not-allowed border border-solid border-sui-gray-50 rounded-md";

function CustomPagination() {
  const { currentRefinement, nbPages, refine, pages } = usePagination();

  if (nbPages <= 1) return null;

  return (
    <ul className="flex gap-2 mt-4 justify-center items-center list-none">
      {nbPages > 2 && (
        <li
          onClick={() => refine(0)}
          className={` ${
            currentRefinement === 0
              ? `${disabledItemStyle}`
              : `${pageItemStyle}`
          }`}
        >
          <div className="rotate-180 flex items-center">{jump}</div>
        </li>
      )}
      {nbPages > 1 && (
        <li
          onClick={() => currentRefinement > 0 && refine(currentRefinement - 1)}
          className={`${
            currentRefinement === 0
              ? `${disabledItemStyle}`
              : `${pageItemStyle}`
          }`}
        >
          <div className="rotate-180 flex items-center">{step}</div>
        </li>
      )}
      {pages.map((page) => {
        const isActive = currentRefinement === page;
        return (
          <li
            key={page}
            onClick={() => refine(page)}
            className={`${pageItemStyle} ${
              isActive
                ? "bg-sui-blue-light/40 dark:bg-sui-blue-light text-sui-blue-dark"
                : ""
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
          className={`${
            currentRefinement === nbPages - 1
              ? `${disabledItemStyle}`
              : `${pageItemStyle}`
          }`}
        >
          <div className="flex items-center">{step}</div>
        </li>
      )}
      {nbPages > 2 && (
        <li
          onClick={() => refine(nbPages - 1)}
          className={`${
            currentRefinement === nbPages - 1
              ? `${disabledItemStyle}`
              : `${pageItemStyle}`
          }`}
        >
          <div className="flex items-center">{jump}</div>
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
