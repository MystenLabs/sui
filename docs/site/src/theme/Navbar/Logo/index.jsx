// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import React from "react";
import Link from "@docusaurus/Link";
import useBaseUrl from "@docusaurus/useBaseUrl";
import useDocusaurusContext from "@docusaurus/useDocusaurusContext";
import { useLocation } from "@docusaurus/router";
import ThemedImage from "@theme/ThemedImage";

function detectLocaleFromPath(pathname) {
  return (
    /^\/([a-z]{2})(?:\/|$)/i.exec(pathname || "")?.[1]?.toLowerCase() ?? null
  );
}

export default function NavbarLogo() {
  const { i18n, siteConfig } = useDocusaurusContext();
  const { defaultLocale } = i18n ?? { defaultLocale: "en" };
  const { pathname } = useLocation();

  const currentLocale = detectLocaleFromPath(pathname) ?? defaultLocale;

  const localizedHome =
    currentLocale && currentLocale !== defaultLocale
      ? `/${currentLocale}/`
      : "/";

  const to = useBaseUrl(localizedHome, { forcePrependBaseUrl: true });

  const logo = siteConfig?.themeConfig?.navbar?.logo ?? {};
  const { src = "img/sui-logo.svg", srcDark, alt = "Logo" } = logo;

  return (
    <Link to={to} className="navbar__brand" aria-label="Homepage">
      <ThemedImage
        className="navbar__logo"
        sources={{
          light: useBaseUrl(src),
          dark: useBaseUrl(srcDark || src),
        }}
        alt={alt}
      />
      {siteConfig?.themeConfig?.navbar?.title && (
        <strong className="navbar__title">
          {siteConfig.themeConfig.navbar.title}
        </strong>
      )}
    </Link>
  );
}
