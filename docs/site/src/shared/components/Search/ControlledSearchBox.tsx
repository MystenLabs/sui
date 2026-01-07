// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React from "react";
import { useSearchBox } from "react-instantsearch";

export default function ControlledSearchBox({
  placeholder,
  query,
  onChange,
  inputRef,
}: {
  placeholder: string;
  query: string;
  onChange: (value: string) => void;
  inputRef?: React.RefObject<HTMLInputElement>;
}) {
  const { refine } = useSearchBox();
  const searchSvg = (
    <svg
      width="24"
      height="24"
      viewBox="0 0 24 24"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
    >
      <path
        d="M11 4C14.866 4 18 7.13401 18 11C18 12.6628 17.4186 14.1888 16.4502 15.3896L19.7803 18.7197L19.832 18.7764C20.0723 19.0709 20.0549 19.5057 19.7803 19.7803C19.5057 20.0549 19.0709 20.0723 18.7764 19.832L18.7197 19.7803L15.3896 16.4502C14.1888 17.4186 12.6628 18 11 18C7.13401 18 4 14.866 4 11C4 7.13401 7.13401 4 11 4ZM11 5.5C7.96243 5.5 5.5 7.96243 5.5 11C5.5 14.0376 7.96243 16.5 11 16.5C14.0376 16.5 16.5 14.0376 16.5 11C16.5 7.96243 14.0376 5.5 11 5.5Z"
        fill="#333333"
      />
    </svg>
  );

  React.useEffect(() => {
    refine(query);
  }, [query, refine]);

  React.useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    if (query) {
      params.set("q", query);
    } else {
      params.delete("q");
    }
    const newUrl = `${window.location.pathname}?${params.toString()}`;
    window.history.replaceState(null, "", newUrl);
  }, [query]);

  return (
    <div className="flex items-center mb-8 border border-sui-gray-40 dark:border-sui-gray-70 rounded-lg bg-sui-gray-40 dark:bg-sui-gray-80 pl-2">
      {searchSvg}
      <input
        ref={inputRef}
        type="search"
        className="cursor-pointer w-full py-2 pr-4 h-[40px] text-lg border-transparent bg-transparent focus-visible:outline-none dark:text-sui-gray-50 dark:placeholder-sui-gray-50"
        placeholder={placeholder}
        value={query}
        onChange={(e) => onChange(e.currentTarget.value)}
      />
    </div>
  );
}
