// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React, { useState, useRef, useEffect, useCallback } from "react";
import BrowserOnly from "@docusaurus/BrowserOnly";
import copy from "copy-text-to-clipboard";
import "./styles.css";

interface PlayMoveEmbedProps {
  code: string;
  title?: string;
  height?: string;
}

/* ---- Copy button ---- */

function CopyCodeButton({ code }: { code: string }) {
  const [copied, setCopied] = useState(false);
  const timeout = useRef<number>();

  const handleCopy = useCallback(() => {
    copy(code);
    setCopied(true);
    clearTimeout(timeout.current);
    timeout.current = window.setTimeout(() => setCopied(false), 2000);
  }, [code]);

  useEffect(() => () => clearTimeout(timeout.current), []);

  return (
    <button
      type="button"
      onClick={handleCopy}
      aria-label={copied ? "Copied" : "Copy code to clipboard"}
      className="playmove-toolbar-btn"
    >
      {copied ? (
        <>
          <i className="fa-regular fa-thumbs-up" style={{ fontSize: 11 }} /> Copied
        </>
      ) : (
        <>
          <i className="fa-regular fa-copy" style={{ fontSize: 11 }} /> Copy
        </>
      )}
    </button>
  );
}

/* ---- Open in Agent button ---- */

const AGENTS = [
  {
    id: "claude",
    title: "Open in Claude",
    url: (prompt: string) =>
      `https://claude.ai/new?q=${encodeURIComponent(prompt)}`,
  },
  {
    id: "chatgpt",
    title: "Open in ChatGPT",
    url: (prompt: string) =>
      `https://chatgpt.com/?q=${encodeURIComponent(prompt)}`,
  },
  {
    id: "gemini",
    title: "Open in Gemini",
    url: (prompt: string) =>
      `https://gemini.google.com/app?q=${encodeURIComponent(prompt)}`,
  },
];

function OpenInAgentButton({ code }: { code: string }) {
  const [isOpen, setIsOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!isOpen) return;
    const handler = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node))
        setIsOpen(false);
    };
    const esc = (e: KeyboardEvent) => {
      if (e.key === "Escape") setIsOpen(false);
    };
    document.addEventListener("mousedown", handler);
    document.addEventListener("keydown", esc);
    return () => {
      document.removeEventListener("mousedown", handler);
      document.removeEventListener("keydown", esc);
    };
  }, [isOpen]);

  const prompt = `Explain this Move code:\n\n\`\`\`move\n${code}\n\`\`\``;

  return (
    <div ref={ref} style={{ position: "relative", display: "inline-flex" }}>
      <button
        type="button"
        className="playmove-toolbar-btn"
        aria-label="Open code in AI agent"
        aria-haspopup="true"
        aria-expanded={isOpen}
        onClick={() => setIsOpen((o) => !o)}
      >
        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
          <path d="M15 4V2" /><path d="M15 16v-2" /><path d="M8 9h2" /><path d="M20 9h2" />
          <path d="M17.8 11.8L19 13" /><path d="M15 9h.01" /><path d="M17.8 6.2L19 5" />
          <path d="M11 6.2L9.7 5" /><path d="M11 11.8L9.7 13" />
          <path d="M8 15h8a2 2 0 0 0 2-2V7a2 2 0 0 0-2-2H8a2 2 0 0 0-2 2v6a2 2 0 0 0 2 2z" />
          <path d="M9 18h6" /><path d="M10 22h4" /><path d="M10 18v4" /><path d="M14 18v4" />
        </svg>
        {" "}Use an Agent
        <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5"
          style={{ transition: "transform 0.2s", transform: isOpen ? "rotate(180deg)" : "none" }}>
          <polyline points="6,9 12,15 18,9" />
        </svg>
      </button>

      {isOpen && (
        <div className="playmove-agent-dropdown">
          {AGENTS.map((agent) => (
            <button
              key={agent.id}
              type="button"
              className="playmove-agent-item"
              onClick={() => {
                window.open(agent.url(prompt), "_blank", "noopener");
                setIsOpen(false);
              }}
            >
              {agent.title}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

/* ---- Main embed ---- */

function PlayMoveIframe({ code, title, height = "600px" }: PlayMoveEmbedProps) {
  const isDark =
    typeof document !== "undefined" &&
    document.documentElement.getAttribute("data-theme") === "dark";
  const theme = isDark ? "dark" : "light";
  const src = `https://www.playmove.dev/?theme=${theme}#${encodeURIComponent(code)}`;

  return (
    <div className="playmove-embed">
      <div className="playmove-toolbar">
        <span className="playmove-title">
          {title || "Move Playground"}
        </span>
        <div className="playmove-actions">
          <CopyCodeButton code={code} />
          <OpenInAgentButton code={code} />
        </div>
      </div>
      <iframe
        src={src}
        width="100%"
        height={height}
        title={title || "Move Playground"}
        className="playmove-iframe"
        allow="clipboard-write"
        sandbox="allow-scripts allow-same-origin allow-popups allow-forms"
      />
      {/* Hidden code element for any external tooling that walks the DOM */}
      <pre style={{ display: "none" }}>
        <code className="language-move">{code}</code>
      </pre>
    </div>
  );
}

export default function PlayMoveEmbed(props: PlayMoveEmbedProps) {
  return (
    <BrowserOnly
      fallback={
        <div className="playmove-embed playmove-loading">
          Loading playground...
        </div>
      }
    >
      {() => <PlayMoveIframe {...props} />}
    </BrowserOnly>
  );
}
