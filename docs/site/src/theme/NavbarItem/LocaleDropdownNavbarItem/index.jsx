// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import React from "react";
import ReactCountryFlag from "react-country-flag";
import useDocusaurusContext from "@docusaurus/useDocusaurusContext";
import useBaseUrl from "@docusaurus/useBaseUrl";
import { useLocation } from "@docusaurus/router";
import Link from "@docusaurus/Link";
import clsx from "clsx";
import styles from "./styles.module.css";

const flagMap = { en: "US", ko: "KR" };

function stripLeadingLocale(pathname, locales) {
  const parts = pathname.split("/");
  if (parts.length > 2 && locales.includes(parts[1])) parts.splice(1, 1);
  const joined = parts.join("/") || "/";
  return joined.startsWith("/") ? joined : `/${joined}`;
}
function detectLocaleFromPath(pathname, fallback) {
  return (
    /^\/([a-z]{2})(?:\/|$)/i.exec(pathname || "")?.[1]?.toLowerCase() ??
    fallback
  );
}

export default function LocaleDropdownNavbarItem() {
  const { i18n } = useDocusaurusContext();
  const { locales, defaultLocale, localeConfigs } = i18n;
  const { pathname } = useLocation();

  const activeLoc = detectLocaleFromPath(pathname, defaultLocale);
  const corePath = stripLeadingLocale(pathname, locales);
  const toBaseUrl = useBaseUrl;

  const items = locales.map((loc) => {
    const localizedPath =
      loc === defaultLocale ? corePath : `/${loc}${corePath}`;
    const to = toBaseUrl(localizedPath, { forcePrependBaseUrl: true });
    return {
      loc,
      to,
      label: localeConfigs?.[loc]?.label ?? loc.toUpperCase(),
      countryCode: flagMap[loc],
    };
  });

  const currentCountry = flagMap[activeLoc];

  return (
    <div
      className={clsx(
        "navbar__item",
        "dropdown",
        "dropdown--hoverable",
        "dropdown--right",
        styles.wrap,
      )}
    >
      <button
        className={clsx("navbar__link", styles.btn)}
        type="button"
        aria-haspopup="true"
        aria-expanded="false"
        aria-label="Languages"
        title={localeConfigs?.[activeLoc]?.label ?? activeLoc.toUpperCase()}
        style={{
          background: "transparent",
          border: "none",
          boxShadow: "none",
          paddingInline: 0,
        }}
      >
        <span
          className="navbar__label"
          style={{
            display: "inline-flex",
            alignItems: "center",
            width: "1.75em",
            height: "1.75em",
          }}
        >
          {currentCountry ? (
            <ReactCountryFlag
              countryCode={currentCountry}
              svg
              style={{ width: "1.75em", height: "1.75em", borderRadius: "3px" }}
              className={styles.flag}
              aria-hidden="true"
            />
          ) : (
            <span style={{ fontWeight: 600 }}>{activeLoc.toUpperCase()}</span>
          )}
        </span>
      </button>

      <ul className="dropdown__menu" role="menu">
        {items.map(({ loc, to, label, countryCode }) => (
          <li key={loc} role="none">
            <Link
              role="menuitem"
              className={clsx("dropdown__link", {
                "dropdown__link--active": loc === activeLoc,
              })}
              to={to}
              aria-label={`Switch to ${label}`}
            >
              {countryCode && (
                <ReactCountryFlag
                  countryCode={countryCode}
                  svg
                  style={{
                    width: "1.25em",
                    height: "1.25em",
                    marginRight: ".5em",
                    verticalAlign: "-2px",
                    borderRadius: "2px",
                  }}
                  aria-hidden="true"
                />
              )}
              {label}
            </Link>
          </li>
        ))}
      </ul>
    </div>
  );
}
