// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import React from "react";
import Link from "@docusaurus/Link";
import { useLocation } from "@docusaurus/router";
import clsx from "clsx";

function detectLocaleFromPath(pathname) {
  return (
    /^\/([a-z]{2})(?:\/|$)/i.exec(pathname || "")?.[1]?.toLowerCase() || null
  );
}
function normalizeAbs(path) {
  return path?.startsWith("/") ? path : `/${path || ""}`;
}

export default function DefaultNavbarItem(props) {
  const { to, href, label, className, ...rest } = props;
  const { pathname } = useLocation();

  if (href && (/^https?:\/\//i.test(href) || href.startsWith("mailto:"))) {
    return (
      <a
        className={clsx("navbar__item navbar__link", className)}
        href={href}
        {...rest}
      >
        {label}
      </a>
    );
  }

  const raw = normalizeAbs(to ?? href ?? "/");
  const currentLocale = detectLocaleFromPath(pathname);
  const defaultLocale =
    (typeof window !== "undefined" &&
      window.__docusaurus?.i18n?.defaultLocale) ||
    "en";

  let target = raw;
  if (
    currentLocale &&
    currentLocale !== defaultLocale &&
    !raw.startsWith(`/${currentLocale}/`) &&
    raw !== `/${currentLocale}`
  ) {
    target = `/${currentLocale}${raw}`;
  }

  return (
    <Link
      className={clsx("navbar__item navbar__link", className)}
      to={target}
      {...rest}
    >
      {label}
    </Link>
  );
}
