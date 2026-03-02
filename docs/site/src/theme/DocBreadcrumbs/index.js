// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React from "react";
import clsx from "clsx";
import { ThemeClassNames } from "@docusaurus/theme-common";
import { useWindowSize } from "@docusaurus/theme-common";
import {
  useSidebarBreadcrumbs,
} from "@docusaurus/plugin-content-docs/client";
import { useHomePageRoute } from "@docusaurus/theme-common/internal";
import Link from "@docusaurus/Link";
import { translate } from "@docusaurus/Translate";
import HomeBreadcrumbItem from "@theme/DocBreadcrumbs/Items/Home";
import DocBreadcrumbsStructuredData from "@theme/DocBreadcrumbs/StructuredData";
import styles from "./styles.module.css";
import { useDoc } from "@docusaurus/plugin-content-docs/client";
import TOC from "@theme/TOC";

/**
 * Safely access doc context. Returns { frontMatter, toc } or defaults
 * when rendered outside a DocProvider (e.g., category index pages).
 */
function useDocSafe() {
  try {
    const doc = useDoc();
    return {
      frontMatter: doc.frontMatter || {},
      toc: doc.toc || [],
    };
  } catch {
    return { frontMatter: {}, toc: [] };
  }
}

function BreadcrumbsItemLink({ children, href, isLast }) {
  const className = "breadcrumbs__link";
  if (isLast) {
    return <span className={className}>{children}</span>;
  }
  return href ? (
    <Link className={className} href={href}>
      <span>{children}</span>
    </Link>
  ) : (
    <span className={className}>{children}</span>
  );
}

function BreadcrumbsItem({ children, active }) {
  return (
    <li
      className={clsx("breadcrumbs__item", {
        "breadcrumbs__item--active": active,
      })}
    >
      {children}
    </li>
  );
}

function MobileTOC({ toc }) {
  if (!Array.isArray(toc) || toc.length === 0) return null;

  return (
    <details className={styles.mobileToc}>
      <summary className={styles.mobileTocSummary}>On this page</summary>
      <TOC toc={toc} />
    </details>
  );
}

export default function DocBreadcrumbs() {
  const breadcrumbs = useSidebarBreadcrumbs();
  const homePageRoute = useHomePageRoute();
  const windowSize = useWindowSize();
  const isMobile = windowSize === "mobile";

  if (!breadcrumbs) {
    return null;
  }

  return (
    <>
      {!isMobile && <DocBreadcrumbsStructuredData breadcrumbs={breadcrumbs} />}
      <nav
        className={clsx(
          ThemeClassNames.docs.docBreadcrumbs,
          styles.breadcrumbsContainer,
        )}
        aria-label={translate({
          id: "theme.docs.breadcrumbs.navAriaLabel",
          message: "Breadcrumbs",
          description: "The ARIA label for the breadcrumbs",
        })}
      >
        <ul className="breadcrumbs">
          {homePageRoute && <HomeBreadcrumbItem />}
          {breadcrumbs.map((item, idx) => {
            const isLast = idx === breadcrumbs.length - 1;
            const href =
              item.type === "category" && item.linkUnlisted
                ? undefined
                : item.href;
            return (
              <BreadcrumbsItem key={idx} active={isLast}>
                <BreadcrumbsItemLink href={href} isLast={isLast}>
                  {item.label}
                </BreadcrumbsItemLink>
              </BreadcrumbsItem>
            );
          })}
        </ul>
      </nav>
    </>
  );
}