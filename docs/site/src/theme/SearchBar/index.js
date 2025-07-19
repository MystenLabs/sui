// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React from "react";
import SearchBar from "@theme-original/SearchBar";

export default class SearchBarWrapper extends React.Component {
  componentDidMount() {
    try {
      window.initCookbook();
    } catch (e) {
      // Gracefully ignore errors if something goes wrong
      console.error("Error initializing Ask Cookbook", e);
    }

    const observer = new MutationObserver(() => {
      const modal = document.querySelector(".DocSearch-Modal");
      const footer =
        document.querySelector(".DocSearch-Footer") ||
        document.querySelector(".DocSearch-Dropdown") || // fallback
        document.querySelector(".DocSearch"); // worst-case fallback

      const input = document.querySelector(".DocSearch-Input");

      if (
        modal &&
        footer &&
        input &&
        !document.getElementById("custom-search-footer-link")
      ) {
        const link = document.createElement("a");
        link.id = "custom-search-footer-link";
        link.textContent = "Go to full search page →";
        link.className = "py-2 pr-4 block text-right text-sm font-bold";
        const updateHref = () => {
          const query = encodeURIComponent(input.value || "");
          link.href = `/search?q=${query}`;
        };
        input.addEventListener("input", updateHref);
        updateHref();
        modal.appendChild(link);
      }
    });

    observer.observe(document.body, { childList: true, subtree: true });
  }

  render() {
    return (
      <>
        <SearchBar {...this.props} />
      </>
    );
  }
}
