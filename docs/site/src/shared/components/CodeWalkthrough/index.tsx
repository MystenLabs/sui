// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React, {
  useState,
  useEffect,
  useRef,
  useCallback,
  useMemo,
} from "react";
import { Highlight } from "prism-react-renderer";
import { usePrismTheme } from "@docusaurus/theme-common";
import { importContentMap } from "@generated-imports/ImportContentMap";
import "./styles.css";

/* ---- Step subcomponent (declarative only, rendered by parent) ---- */

interface StepProps {
  lines: string;
  title?: string;
  children: React.ReactNode;
}

export function Step(_props: StepProps) {
  return null;
}

/* ---- Line range parsing ---- */

function parseLines(spec: string): Set<number> {
  const set = new Set<number>();
  for (const part of spec.split(",")) {
    const trimmed = part.trim();
    if (trimmed.includes("-")) {
      const [a, b] = trimmed.split("-").map(Number);
      if (!isNaN(a) && !isNaN(b)) {
        for (let i = a; i <= b; i++) set.add(i);
      }
    } else {
      const n = Number(trimmed);
      if (!isNaN(n)) set.add(n);
    }
  }
  return set;
}

/* ---- Main component ---- */

interface CodeWalkthroughProps {
  source: string;
  org?: string;
  repo?: string;
  branch?: string;
  language?: string;
  children: React.ReactNode;
}

export default function CodeWalkthrough({
  source,
  org,
  repo,
  branch = "main",
  language,
  children,
}: CodeWalkthroughProps) {
  const prismTheme = usePrismTheme();
  const [code, setCode] = useState<string | null>(null);
  const [activeStep, setActiveStep] = useState(0);
  const stepRefs = useRef<(HTMLDivElement | null)[]>([]);
  const codeRef = useRef<HTMLPreElement | null>(null);

  // Extract Step children
  const steps = useMemo(() => {
    const result: StepProps[] = [];
    React.Children.forEach(children, (child) => {
      if (React.isValidElement(child) && child.type === Step) {
        result.push(child.props as StepProps);
      }
    });
    return result;
  }, [children]);

  // Resolve language from file extension
  const resolvedLang = useMemo(() => {
    if (language) return language;
    const ext = source.match(/\.([^.]+)$/)?.[1];
    if (ext === "move") return "move";
    if (ext === "ts" || ext === "tsx") return "typescript";
    if (ext === "rs") return "rust";
    return ext || "text";
  }, [source, language]);

  // Fetch code
  useEffect(() => {
    let cancelled = false;
    async function load() {
      if (org && repo) {
        const path = source.replace(/^\.\/?/, "").replace(/^\//, "");
        const url = `https://raw.githubusercontent.com/${org}/${repo}/${branch}/${path}`;
        try {
          const res = await fetch(url);
          if (!res.ok) throw new Error(`${res.status}`);
          const text = await res.text();
          if (!cancelled) setCode(text);
        } catch {
          if (!cancelled) setCode(`// Failed to load ${source}`);
        }
      } else {
        const cleaned = source.replace(/^\/+/, "").replace(/^\.\//, "");
        const content = importContentMap[cleaned];
        setCode(content ?? `// File not found: ${source}`);
      }
    }
    load();
    return () => { cancelled = true; };
  }, [source, org, repo, branch]);

  // Strip license header
  const cleanCode = useMemo(() => {
    if (!code) return "";
    return code
      .replace(/^\/\/\s*Copyright.*Mysten Labs.*\n\/\/\s*SPDX-License.*?\n?$/gim, "")
      .replace(/^\s*\n/, "");
  }, [code]);

  // Active highlight lines
  const highlightedLines = useMemo(() => {
    if (steps.length === 0) return new Set<number>();
    return parseLines(steps[activeStep]?.lines ?? "");
  }, [steps, activeStep]);

  // IntersectionObserver for step detection
  useEffect(() => {
    const observers: IntersectionObserver[] = [];
    stepRefs.current.forEach((el, i) => {
      if (!el) return;
      const observer = new IntersectionObserver(
        ([entry]) => {
          if (entry.isIntersecting) setActiveStep(i);
        },
        { rootMargin: "-30% 0px -50% 0px", threshold: 0 },
      );
      observer.observe(el);
      observers.push(observer);
    });
    return () => observers.forEach((o) => o.disconnect());
  }, [steps.length, code]);

  // Scroll code panel to keep the full highlighted range visible
  useEffect(() => {
    if (!codeRef.current || highlightedLines.size === 0) return;
    const firstLine = Math.min(...highlightedLines);
    const lastLine = Math.max(...highlightedLines);
    const firstEl = codeRef.current.querySelector(
      `[data-line="${firstLine}"]`,
    ) as HTMLElement | null;
    const lastEl = codeRef.current.querySelector(
      `[data-line="${lastLine}"]`,
    ) as HTMLElement | null;
    if (!firstEl) return;

    const container = codeRef.current;
    const containerRect = container.getBoundingClientRect();
    const firstRect = firstEl.getBoundingClientRect();
    const lastRect = (lastEl ?? firstEl).getBoundingClientRect();

    const rangeTop = firstRect.top - containerRect.top + container.scrollTop;
    const rangeBottom = lastRect.bottom - containerRect.top + container.scrollTop;
    const rangeHeight = rangeBottom - rangeTop;

    // Center the range in the container, or align to top if range is taller than container
    const targetScroll = rangeHeight > containerRect.height
      ? rangeTop
      : rangeTop - (containerRect.height - rangeHeight) / 2;

    container.scrollTo({ top: Math.max(0, targetScroll), behavior: "smooth" });
  }, [highlightedLines]);

  if (!code) {
    return <div className="cw-loading">Loading code...</div>;
  }

  const title = org
    ? `${org}/${repo}/${source.replace(/^\//, "")}`
    : source.replace(/^\/+/, "");

  return (
    <div className="cw-container">
      {/* Left: scrollable steps */}
      <div className="cw-steps">
        {steps.map((step, i) => (
          <div
            key={i}
            ref={(el) => { stepRefs.current[i] = el; }}
            className={`cw-step ${i === activeStep ? "cw-step--active" : ""}`}
            onClick={() => setActiveStep(i)}
          >
            {step.title && <h4 className="cw-step-title">{step.title}</h4>}
            <div className="cw-step-content">{step.children}</div>
          </div>
        ))}
      </div>

      {/* Right: sticky code panel */}
      <div className="cw-code-panel">
        <div className="cw-code-sticky">
          <div className="cw-code-title">{title}</div>
          <pre ref={codeRef} className="cw-code-pre thin-scrollbar">
            <Highlight theme={prismTheme} code={cleanCode} language={resolvedLang}>
              {({ className, style, tokens, getLineProps, getTokenProps }) => (
                <code className={className} style={{ ...style, background: "transparent" }}>
                  {tokens.map((line, i) => {
                    const lineNum = i + 1;
                    const isHighlighted = highlightedLines.has(lineNum);
                    const isDimmed = highlightedLines.size > 0 && !isHighlighted;
                    const lineProps = getLineProps({ line, key: i });
                    return (
                      <div
                        {...lineProps}
                        key={i}
                        data-line={lineNum}
                        className={`cw-line ${isHighlighted ? "cw-line--highlighted" : ""} ${isDimmed ? "cw-line--dimmed" : ""}`}
                      >
                        <span className="cw-line-number">{lineNum}</span>
                        <span className="cw-line-content">
                          {line.map((token, j) => (
                            <span key={j} {...getTokenProps({ token, key: j })} />
                          ))}
                        </span>
                      </div>
                    );
                  })}
                </code>
              )}
            </Highlight>
          </pre>
        </div>
      </div>
    </div>
  );
}
