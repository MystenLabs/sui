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
      const footer = document.querySelector(".DocSearch-HitsFooter");
      const input = document.querySelector(".DocSearch-Input");
      if (
        footer &&
        input &&
        !document.getElementById("custom-search-footer-link")
      ) {
        const link = document.createElement("a");
        link.id = "custom-search-footer-link";
        link.textContent = "Go to full search page →";
        link.style.cssText =
          "margin-top: 8px; display: block; font-weight: bold;";
        const updateHref = () => {
          const query = encodeURIComponent(input.value || "");
          link.href = `/search?q=${query}`;
        };
        input.addEventListener("input", updateHref);
        updateHref();
        footer.appendChild(link);
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
