// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import React from "react";
import useDocusaurusContext from "@docusaurus/useDocusaurusContext";
import useBaseUrl from "@docusaurus/useBaseUrl";
import { useLocation } from "@docusaurus/router";
import Link from "@docusaurus/Link";
import IconLanguage from "@theme/Icon/Language";
import clsx from "clsx";

function stripLeadingLocale(pathname, locales) {
  const parts = pathname.split("/");
  if (parts.length > 2 && locales.includes(parts[1])) parts.splice(1, 1);
  const joined = parts.join("/") || "/";
  return joined.startsWith("/") ? joined : `/${joined}`;
}

export default function LocaleDropdownNavbarItem() {
  const { i18n } = useDocusaurusContext();
  const { locales, defaultLocale, localeConfigs, currentLocale } = i18n;
  const { pathname } = useLocation();

  const baseUrlFn = useBaseUrl;

  const corePath = stripLeadingLocale(pathname, locales);

  const items = locales.map((loc) => {
    const localizedPath =
      loc === defaultLocale ? corePath : `/${loc}${corePath}`;
    const to = baseUrlFn(localizedPath, { forcePrependBaseUrl: true });
    return { loc, to, label: localeConfigs?.[loc]?.label ?? loc.toUpperCase() };
  });

  const currentLabel =
    localeConfigs?.[currentLocale]?.label ??
    currentLocale?.toUpperCase?.() ??
    "EN";

  const activeLoc =
    /^\/([a-z]{2})(?:\/|$)/i.exec(pathname)?.[1] ?? defaultLocale;

  return (
    <div className="navbar__item dropdown dropdown--hoverable dropdown--right">
      <button
        className="navbar__link"
        type="button"
        aria-haspopup="true"
        aria-expanded="false"
        aria-label="Languages"
        style={{ background: "transparent", border: "none", boxShadow: "none" }}
      >
        <IconLanguage className="navbar__icon" />
        <span className="navbar__label">{currentLabel}</span>
        <span className="navbar__link--caret" />
      </button>
      <ul className="dropdown__menu">
        {items.map(({ loc, to, label }) => (
          <li key={loc}>
            <Link
              className={clsx("dropdown__link", {
                "dropdown__link--active": loc === activeLoc,
              })}
              to={to}
            >
              {label}
            </Link>
          </li>
        ))}
      </ul>
    </div>
  );
}
