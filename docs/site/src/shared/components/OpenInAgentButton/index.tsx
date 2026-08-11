// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/**
 * OpenInAgentButton — shared dropdown that lets users send a code snippet to an
 * AI assistant (Claude, ChatGPT, Gemini). Designed to sit inside a Docusaurus
 * CodeBlock button bar.
 *
 * Usage (in a swizzled CodeBlock/Buttons/index.js):
 *
 *   import OpenInAgentButton from "../../shared/components/OpenInAgentButton";
 *   // … render <OpenInAgentButton /> alongside <CopyButton /> etc.
 *
 * The component walks the DOM upward from its own position to locate the
 * nearest <pre><code> element, extracts the text content and language, and
 * opens the chosen agent in a new tab with a pre-filled prompt.
 */
import React, {
  useState,
  useRef,
  useEffect,
  useCallback,
  type ReactNode,
} from "react";
import clsx from "clsx";
import styles from "./styles.module.css";

/* ---------- helpers ---------- */

/** Walk up from `start` to find the nearest code text in the code block. */
function getNearestCodeText(start: HTMLElement | null): string {
  let el: HTMLElement | null = start;
  while (el) {
    const code = el.querySelector?.(
      "pre code, code, pre",
    ) as HTMLElement | null;
    if (code?.innerText) {
      return code.innerText
        .replace(/^\$ /gm, "")
        .replace(/\n$/, "");
    }
    el = el.parentElement;
  }
  return "";
}

/** Try to detect the language label from the nearest code block. */
function getNearestLanguage(start: HTMLElement | null): string {
  let el: HTMLElement | null = start;
  while (el) {
    const code = el.querySelector?.("code") as HTMLElement | null;
    const match = code?.className?.match(/language-(\w+)/);
    if (match) return match[1];
    el = el.parentElement;
  }
  return "";
}

/* ---------- SVG icon paths (extracted to stay under line limits) ------- */

const CLAUDE_PATH = [
  "M4.709 15.955l4.72-2.647.08-.23-.08-.128H9.2l-.79-.048",
  "-2.698-.073-2.339-.097-2.266-.122-.571-.121L0 11.784",
  "l.055-.352.48-.321.686.06 1.52.103 2.278.158 1.652",
  ".097 2.449.255h.389l.055-.157-.134-.098-.103-.097",
  "-2.358-1.596-2.552-1.688-1.336-.972-.724-.491-.364",
  "-.462-.158-1.008.656-.722.881.06.225.061.893.686",
  " 1.908 1.476 2.491 1.833.365.304.145-.103.019-.073",
  "-.164-.274-1.355-2.446-1.446-2.49-.644-1.032-.17",
  "-.619a2.97 2.97 0 01-.104-.729L6.283.134 6.696 0",
  "l.996.134.42.364.62 1.414 1.002 2.229 1.555 3.03",
  ".456.898.243.832.091.255h.158V9.01l.128-1.706.237",
  "-2.095.23-2.695.08-.76.376-.91.747-.492.584.28.48",
  ".685-.067.444-.286 1.851-.559 2.903-.364 1.942h.212",
  "l.243-.242.985-1.306 1.652-2.064.73-.82.85-.904",
  ".547-.431h1.033l.76 1.129-.34 1.166-1.064 1.347",
  "-.881 1.142-1.264 1.7-.79 1.36.073.11.188-.02",
  " 2.856-.606 1.543-.28 1.841-.315.833.388.091.395",
  "-.328.807-1.969.486-2.309.462-3.439.813-.042.03",
  ".049.061 1.549.146.662.036h1.622l3.02.225.79.522",
  ".474.638-.079.485-1.215.62-1.64-.389-3.829-.91",
  "-1.312-.329h-.182v.11l1.093 1.068 2.006 1.81 2.509",
  " 2.33.127.578-.322.455-.34-.049-2.205-1.657-.851",
  "-.747-1.926-1.62h-.128v.17l.444.649 2.345 3.521",
  ".122 1.08-.17.353-.608.213-.668-.122-1.374-1.925",
  "-1.415-2.167-1.143-1.943-.14.08-.674 7.254-.316",
  ".37-.729.28-.607-.461-.322-.747.322-1.476.389-1.924",
  ".315-1.53.286-1.9.17-.632-.012-.042-.14.018-1.434",
  " 1.967-2.18 2.945-1.726 1.845-.414.164-.717-.37",
  ".067-.662.401-.589 2.388-3.036 1.44-1.882.93-1.086",
  "-.006-.158h-.055L4.132 18.56l-1.13.146-.487-.456",
  ".061-.746.231-.243 1.908-1.312-.006.006z",
].join("");

const CHATGPT_PATH = [
  "M37.532 16.87a9.963 9.963 0 0 0-.856-8.184 10.078",
  " 10.078 0 0 0-10.855-4.835A9.964 9.964 0 0 0",
  " 18.306.5a10.079 10.079 0 0 0-9.614 6.977 9.967",
  " 9.967 0 0 0-6.664 4.834 10.08 10.08 0 0 0 1.24",
  " 11.817 9.965 9.965 0 0 0 .856 8.185 10.079 10.079",
  " 0 0 0 10.855 4.835 9.965 9.965 0 0 0 7.516 3.35",
  " 10.078 10.078 0 0 0 9.617-6.981 9.967 9.967 0 0 0",
  " 6.663-4.834 10.079 10.079 0 0 0-1.243-11.813z",
  "M22.498 37.886a7.474 7.474 0 0 1-4.799-1.735c.061",
  "-.033.168-.091.237-.134l7.964-4.6a1.294 1.294 0 0 0",
  " .655-1.134V19.054l3.366 1.944a.12.12 0 0 1 .066",
  ".092v9.299a7.505 7.505 0 0 1-7.49 7.496zM6.392",
  " 31.006a7.471 7.471 0 0 1-.894-5.023c.06.036.162",
  ".099.237.141l7.964 4.6a1.297 1.297 0 0 0 1.308",
  " 0l9.724-5.614v3.888a.12.12 0 0 1-.048.103l-8.051",
  " 4.649a7.504 7.504 0 0 1-10.24-2.744zM4.297 13.62",
  "A7.469 7.469 0 0 1 8.2 10.333c0 .068-.004.19-.004",
  ".274v9.201a1.294 1.294 0 0 0 .654 1.132l9.723",
  " 5.614-3.366 1.944a.12.12 0 0 1-.114.01L7.04",
  " 23.856a7.504 7.504 0 0 1-2.743-10.237zm27.658",
  " 6.437l-9.724-5.615 3.367-1.943a.121.121 0 0 1",
  " .113-.01l8.052 4.648a7.498 7.498 0 0 1-1.158",
  " 13.528v-9.476a1.293 1.293 0 0 0-.65-1.132zm3.35",
  "-5.043c-.059-.037-.162-.099-.236-.141l-7.965-4.6",
  "a1.298 1.298 0 0 0-1.308 0l-9.723 5.614v-3.888",
  "a.12.12 0 0 1 .048-.103l8.05-4.645a7.497 7.497",
  " 0 0 1 11.135 7.763zm-21.063 6.929l-3.367-1.944",
  "a.12.12 0 0 1-.065-.092v-9.299a7.497 7.497 0 0 1",
  " 12.293-5.756 6.94 6.94 0 0 0-.236.134l-7.965",
  " 4.6a1.294 1.294 0 0 0-.654 1.132l-.006 11.225z",
  "m1.829-3.943l4.33-2.501 4.332 2.5v5l-4.331 2.5",
  "-4.331-2.5V18z",
].join("");

const GEMINI_PATH = [
  "M12 0C12 0 12 6.268 8.134 10.134",
  "C4.268 14 0 14 0 14C0 14 4.268 14 8.134 17.866",
  "C12 21.732 12 28 12 28C12 28 12 21.732 15.866",
  " 17.866C19.732 14 24 14 24 14C24 14 19.732 14",
  " 15.866 10.134C12 6.268 12 0 12 0Z",
].join("");

/* ---------- prompt presets ---------- */

/**
 * Prompt presets configurable per code block via the `agentPrompt` metastring.
 *
 * Usage in markdown:
 *   ```move agentPrompt="build"
 *   module hello::world { ... }
 *   ```
 *
 * Add new presets here as needed. The key is the value authors pass in
 * `agentPrompt="<key>"`. The value is a function receiving (code, lang)
 * that returns the full prompt string.
 */
const PROMPT_PRESETS: Record<string, (code: string, lang: string) => string> = {
  explain: (code, lang) => {
    const l = lang ? ` ${lang}` : "";
    return `Explain this${l} code:\n\n\`\`\`${lang}\n${code}\n\`\`\``;
  },
  build: (code, lang) =>
    `Use this prompt for building on Sui:\n\n\`\`\`${lang}\n${code}\n\`\`\``,
};

const DEFAULT_PRESET = "explain";

/** Walk up from `start` to find the nearest `data-agent-prompt` attribute. */
function getNearestPromptPreset(start: HTMLElement | null): string {
  let el: HTMLElement | null = start;
  while (el) {
    const preset = el.getAttribute("data-agent-prompt");
    if (preset) return preset;
    el = el.parentElement;
  }
  return DEFAULT_PRESET;
}

function buildPrompt(code: string, lang: string, preset: string): string {
  const fn = PROMPT_PRESETS[preset] ?? PROMPT_PRESETS[DEFAULT_PRESET];
  return fn(code, lang);
}

/* ---------- agent definitions ---------- */

interface AgentItem {
  id: string;
  title: string;
  description: string;
  icon: ReactNode;
  url: (code: string, lang: string, preset: string) => string;
}

const AGENTS: AgentItem[] = [
  {
    id: "claude",
    title: "Open in Claude",
    description: "Explain or modify this snippet",
    icon: (
      <svg
        width="16"
        height="16"
        fill="currentColor"
        fillRule="evenodd"
        xmlns="http://www.w3.org/2000/svg"
        viewBox="0 0 24 24"
      >
        <path d={CLAUDE_PATH} />
      </svg>
    ),
    url: (code, lang, preset) =>
      `https://claude.ai/new?q=${encodeURIComponent(buildPrompt(code, lang, preset))}`,
  },
  {
    id: "chatgpt",
    title: "Open in ChatGPT",
    description: "Explain or modify this snippet",
    icon: (
      <svg
        width="16"
        height="16"
        fill="currentColor"
        xmlns="http://www.w3.org/2000/svg"
        strokeWidth="1.5"
        viewBox="-0.17 0.48 41.14 40.03"
      >
        <path d={CHATGPT_PATH} />
      </svg>
    ),
    url: (code, lang, preset) =>
      `https://chatgpt.com/?q=${encodeURIComponent(buildPrompt(code, lang, preset))}`,
  },
  {
    id: "gemini",
    title: "Open in Gemini",
    description: "Explain or modify this snippet",
    icon: (
      <svg
        width="16"
        height="16"
        fill="currentColor"
        xmlns="http://www.w3.org/2000/svg"
        viewBox="0 0 24 24"
      >
        <path
          d={GEMINI_PATH}
          transform="scale(0.857) translate(0, -2)"
        />
      </svg>
    ),
    url: (code, lang, preset) =>
      `https://gemini.google.com/app?q=${encodeURIComponent(buildPrompt(code, lang, preset))}`,
  },
];

/* ---------- component ---------- */

export default function OpenInAgentButton({
  className,
  ButtonComponent,
}: {
  className?: string;
  /** Optional base button component from the site's theme (e.g. @theme/CodeBlock/Buttons/Button).
   *  Falls back to a plain <button> when not provided. */
  ButtonComponent?: React.ComponentType<React.ButtonHTMLAttributes<HTMLButtonElement>>;
}): ReactNode {
  const [isOpen, setIsOpen] = useState(false);
  const wrapperRef = useRef<HTMLDivElement | null>(null);

  const Btn = ButtonComponent ?? "button";

  // Close on outside click
  useEffect(() => {
    if (!isOpen) return;
    const handler = (e: MouseEvent) => {
      if (wrapperRef.current && !wrapperRef.current.contains(e.target as Node)) {
        setIsOpen(false);
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [isOpen]);

  // Close on Escape
  useEffect(() => {
    if (!isOpen) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") setIsOpen(false);
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [isOpen]);

  const handleAgentClick = useCallback(
    (agent: AgentItem) => {
      const code = getNearestCodeText(wrapperRef.current);
      const lang = getNearestLanguage(wrapperRef.current);
      const preset = getNearestPromptPreset(wrapperRef.current);
      if (!code) return;
      window.open(agent.url(code, lang, preset), "_blank", "noopener");
      setIsOpen(false);
    },
    [],
  );

  return (
    <div ref={wrapperRef} className={styles.wrapper}>
      <Btn
        type="button"
        className={clsx(className, styles.triggerBtn, "clean-btn")}
        aria-label="Open code in AI agent"
        aria-haspopup="true"
        aria-expanded={isOpen}
        title="Open in agent"
        onClick={() => setIsOpen((o) => !o)}
      >
        <svg
          width="14"
          height="14"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
        >
          <path d="M15 4V2" />
          <path d="M15 16v-2" />
          <path d="M8 9h2" />
          <path d="M20 9h2" />
          <path d="M17.8 11.8L19 13" />
          <path d="M15 9h.01" />
          <path d="M17.8 6.2L19 5" />
          <path d="M11 6.2L9.7 5" />
          <path d="M11 11.8L9.7 13" />
          <path d="M8 15h8a2 2 0 0 0 2-2V7a2 2 0 0 0-2-2H8a2 2 0 0 0-2 2v6a2 2 0 0 0 2 2z" />
          <path d="M9 18h6" />
          <path d="M10 22h4" />
          <path d="M10 18v4" />
          <path d="M14 18v4" />
        </svg>
        <span className={styles.label}>Use an Agent</span>
        <svg
          width="10"
          height="10"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2.5"
          className={clsx(styles.chevron, isOpen && styles.chevronOpen)}
        >
          <polyline points="6,9 12,15 18,9" />
        </svg>
      </Btn>

      {isOpen && (
        <div className={styles.dropdown}>
          {AGENTS.map((agent) => (
            <button
              key={agent.id}
              type="button"
              className={styles.item}
              onClick={() => handleAgentClick(agent)}
            >
              {agent.icon}
              <div>
                <div className={styles.itemTitle}>{agent.title}</div>
                <div className={styles.itemDesc}>{agent.description}</div>
              </div>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
