
import React from "react";
import clsx from "clsx";
import { ThemeClassNames } from "@docusaurus/theme-common";
import { useThemeConfig } from "@docusaurus/theme-common";
import { useWindowSize } from "@docusaurus/theme-common";
import {
  useSidebarBreadcrumbs,
  useDocsSidebar,
} from "@docusaurus/plugin-content-docs/client";
import { useHomePageRoute } from "@docusaurus/theme-common/internal";
import Link from "@docusaurus/Link";
import { useLocation } from "@docusaurus/router";
import { translate } from "@docusaurus/Translate";
import HomeBreadcrumbItem from "@theme/DocBreadcrumbs/Items/Home";
import DocBreadcrumbsStructuredData from "@theme/DocBreadcrumbs/StructuredData";
import styles from "./styles.module.css";
import { useDoc } from "@docusaurus/plugin-content-docs/client";
import TOC from "@theme/TOC";
// TODO move to design system folder

const normalizeUrl = (u) => {
  if (!u || typeof u !== "string") return undefined;
  if (u.startsWith("http://") || u.startsWith("https://") || u.startsWith("/"))
    return u;
  return "/" + u.replace(/^\./, "");
};

const normalizePath = (p) => {
  if (!p) return "/";
  try {
    const u = new URL(p, "http://_");
    const path = u.pathname || "/";
    return path === "/" ? "/" : path.replace(/\/+$/, "");
  } catch {
    const path = p.startsWith("/") ? p : "/" + p;
    return path === "/" ? "/" : path.replace(/\/+$/, "");
  }
};

// Local mobile site/section dropdown (no external component needed)
function MobileSidebarSelectInline() {
  const sidebar = useDocsSidebar();
  const location = useLocation();
  const [open, setOpen] = React.useState(false);

  const themeConfig = useThemeConfig();
  const navbarItems = Array.isArray(themeConfig?.navbar?.items)
    ? themeConfig.navbar.items
    : [];
  // Extract only direct link items (ignore dropdowns) and normalize href
  const topNavLinksAll = navbarItems
    .map((it) => {
      const raw = it.href ?? it.to;
      const url = normalizeUrl(raw);
      const label = it.label;
      if (!url || !label) return null;
      const external = /^https?:\/\//.test(url);
      return { url, label, external };
    })
    .filter(Boolean);
  // Determine the current top-level nav by prefix match
  const currentPath = location?.pathname || "";
  const currentTop = topNavLinksAll.find(
    (it) => currentPath === it.url || currentPath.startsWith(it.url + "/"),
  );

  if (!sidebar || !Array.isArray(sidebar.items) || sidebar.items.length === 0) {
    return null;
  }

  // Build a flat list including categories (as headings) and links, with depth for indentation
  const flatten = (items, depth = 0) => {
    const out = [];
    for (const it of items) {
      if (!it) continue;
      if (it.type === "category") {
        // Include the category itself (even if it has no href)
        out.push({
          type: "category",
          label: it.label,
          href: it.href, // might be undefined if not linkable/generated-index
          depth,
        });
        // Recurse into children
        out.push(...flatten(it.items || [], depth + 1));
      } else if (it.type === "link" && it.href) {
        out.push({ type: "link", label: it.label, href: it.href, depth });
      }
    }
    return out;
  };

  const links = flatten(sidebar.items);
  if (links.length === 0) return null;

  const normalizedCurrent = normalizePath(location?.pathname || "");

  return (
    <div className="mobileSidebarSelect" style={{ margin: "0.75rem 0 0.5rem" }}>
      <button
        type="button"
        className="p-2 border-lg flex h-8"
        onClick={() => setOpen(!open)}
        aria-label="Toggle site navigation"
      >
        <span
          className=""
          style={{
            display: "inline-block",
            width: 20,
            height: 2,
            background: "currentColor",
            boxShadow: "0 6px currentColor, 0 12px currentColor",
          }}
        />
      </button>
      {open && (
        <div className="mobileSidebarLinks" style={{ marginTop: "0.5rem" }}>
          {topNavLinksAll.length > 0 && (
            <div style={{ marginBottom: "0.5rem" }}>
              {topNavLinksAll.map((item) => (
                <div key={`top-${item.url}`} style={{ padding: "0.25rem 0" }}>
                  {currentTop && item.url === currentTop.url ? (
                    <div className="font-bold">{item.label}</div>
                  ) : item.external ? (
                    <Link
                      href={item.url}
                      onClick={() => setOpen(false)}
                      style={{ fontWeight: 600 }}
                    >
                      {item.label}
                    </Link>
                  ) : (
                    <Link
                      to={item.url}
                      onClick={() => setOpen(false)}
                      style={{ fontWeight: 600 }}
                    >
                      {item.label}
                    </Link>
                  )}
                </div>
              ))}
              <div
                style={{
                  height: 1,
                  background: "var(--ifm-toc-border-color, #eaecef)",
                  margin: "0.25rem 0",
                }}
              />
            </div>
          )}
          {links.map((item) => {
            const isCurrent = normalizePath(item.href) === normalizedCurrent;
            return (
              <div
                key={`${item.type}-${item.href || item.label}`}
                style={{
                  padding: "0.25rem 0",
                  paddingLeft: `${item.depth * 16}px`,
                }}
              >
                {item.href ? (
                  isCurrent ? (
                    <div className="font-bold">{item.label}</div>
                  ) : (
                    <Link
                      to={item.href}
                      onClick={() => setOpen(false)}
                      style={{ fontWeight: 400 }}
                    >
                      {item.label}
                    </Link>
                  )
                ) : (
                  <span style={{ opacity: 0.8, fontWeight: 600 }}>
                    {item.label}
                  </span>
                )}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
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
// TODO move to design system folder
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
export default function DocBreadcrumbs() {
  const breadcrumbs = useSidebarBreadcrumbs();
  const homePageRoute = useHomePageRoute();
  const { frontMatter, toc } = useDoc();
  const windowSize = useWindowSize();
  const isMobile = windowSize === "mobile";
  const showTOC =
    !frontMatter?.hide_table_of_contents &&
    Array.isArray(toc) &&
    toc.length > 0;
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
        {isMobile ? (
          <>
            {/* Mobile: show site navigation dropdown above the ToC dropdown */}
            <MobileSidebarSelectInline />
            {showTOC && (
              <div className="breadcrumbs__container--toc">
                <TOC toc={toc} />
              </div>
            )}
          </>
        ) : (
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
        )}
      </nav>
    </>
  );
}
