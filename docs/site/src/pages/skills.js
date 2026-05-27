// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React, { useMemo, useState } from "react";

import Layout from "@theme/Layout";
import Link from "@docusaurus/Link";

import styles from "./skills.module.css";

// Self-contained install command with a copy button. The page deliberately
// avoids @theme/CodeBlock so it does not depend on the site's CodeBlock theme
// swizzle.
function CopyCommand({ command }) {
  const [copied, setCopied] = useState(false);
  return (
    <div className={styles.install}>
      <code className={styles.installCmd}>{command}</code>
      <button
        type="button"
        className={`${styles.copyBtn} plausible-event-name=skills+copy+install`}
        onClick={() => {
          if (typeof navigator !== "undefined" && navigator.clipboard) {
            navigator.clipboard.writeText(command);
          }
          setCopied(true);
          setTimeout(() => setCopied(false), 1500);
        }}
      >
        {copied ? "Copied" : "Copy"}
      </button>
    </div>
  );
}

// Skill data is generated at build time from the mystenlabs/skills repository
// by scripts/generate-skills.mjs (wired into the prebuild and prestart npm
// scripts). Do not edit src/data/skills.json by hand: it regenerates on every
// build, so the page stays in sync with the repo automatically.
import SKILLS from "../data/skills.json";

const REPO = "https://github.com/MystenLabs/skills";

// Categories are derived from the generated data, so new categories appear
// automatically. Sorted alphabetically, with "Get started" floated first.
function orderedCategories(skills) {
  const unique = [...new Set(skills.map((skill) => skill.category))].sort(
    (a, b) => a.localeCompare(b),
  );
  return unique.sort((a, b) => {
    const first = /^get(ting)? started$/i;
    return first.test(a) ? -1 : first.test(b) ? 1 : 0;
  });
}

export default function Skills() {
  const [active, setActive] = useState("All");

  const categories = useMemo(() => orderedCategories(SKILLS), []);
  const counts = useMemo(() => {
    const map = { All: SKILLS.length };
    for (const category of categories) {
      map[category] = SKILLS.filter((s) => s.category === category).length;
    }
    return map;
  }, [categories]);

  const visible =
    active === "All"
      ? SKILLS
      : SKILLS.filter((skill) => skill.category === active);

  return (
    <Layout
      title="Sui Agent Skills"
      description="Pre-built agent skills for building on Sui. Install them into Claude Code, Cursor, Codex, and other AI coding agents."
    >
      <div className={styles.page}>
        <header className={styles.hero}>
          <h1 className={styles.heroTitle}>Sui Agent Skills</h1>
          <p className={styles.heroTagline}>
            Pre-built skills you can drop into your AI coding agent to build on
            Sui. Install them into Claude Code, Cursor, Codex, and other agents
            with the <code>skills</code> CLI.
          </p>
          <CopyCommand command="npx skills add mystenlabs/skills --all" />
          <Link className={styles.repoLink} to={REPO}>
            View the repository on GitHub →
          </Link>
        </header>

        {categories.length > 1 && (
          <div className={styles.controls}>
            {["All", ...categories].map((category) => (
              <button
                key={category}
                type="button"
                className={`${styles.chip} ${
                  active === category ? styles.chipActive : ""
                }`}
                onClick={() => setActive(category)}
              >
                {category}
                <span className={styles.chipCount}>{counts[category]}</span>
              </button>
            ))}
          </div>
        )}

        <p className={styles.installHint}>
          Install any single skill with{" "}
          <code>npx skills add mystenlabs/skills --skill &lt;name&gt;</code>.
        </p>

        <div className={styles.grid}>
          {visible.map((skill) => (
            <Link
              key={skill.slug}
              className={styles.card}
              to={`${REPO}/tree/main/${skill.path || skill.slug}`}
            >
              <span className={styles.cardCategory}>{skill.category}</span>
              <h2 className={styles.cardTitle}>{skill.title}</h2>
              <p className={styles.cardDesc}>{skill.description}</p>
              <div className={styles.cardFooter}>
                <span className={styles.cardSlug}>{skill.slug}</span>
                <span className={styles.cardLink}>View on GitHub →</span>
              </div>
            </Link>
          ))}
        </div>
      </div>
    </Layout>
  );
}
