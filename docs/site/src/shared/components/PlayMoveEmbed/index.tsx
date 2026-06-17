// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React from "react";
import "./styles.css";

interface PlayMoveEmbedProps {
  code: string;
  title?: string;
  children: React.ReactNode;
}

export default function PlayMoveEmbed({ code, children }: PlayMoveEmbedProps) {
  const handleOpen = () => {
    const isDark =
      typeof document !== "undefined" &&
      document.documentElement.getAttribute("data-theme") === "dark";
    const url = `https://www.playmove.dev/?theme=${isDark ? "dark" : "light"}#${encodeURIComponent(code)}`;
    window.open(url, "_blank", "noopener");
  };

  return (
    <div className="playmove-wrapper">
      {children}
      <button
        type="button"
        className="playmove-open-btn"
        onClick={handleOpen}
      >
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
          <polygon points="5,3 19,12 5,21" />
        </svg>
        Open in PlayMove
      </button>
    </div>
  );
}
