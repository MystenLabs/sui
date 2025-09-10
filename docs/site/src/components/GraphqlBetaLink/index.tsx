// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React, { useEffect, useState } from "react";
import { useLocation } from "@docusaurus/router";
import { matchPath } from "@docusaurus/router";
import routes from "@generated/routes";

function routeExists(candidate: string, routeTree: any[]): boolean {
  for (const r of routeTree) {
    // Docusaurus routes have `path`, `exact`, and optional nested `routes`
    if (r.path) {
      const m =
        r.path !== "*" && matchPath(candidate, { path: r.path, exact: true });
      if (m) return true;
    }
    if (r.routes && routeExists(candidate, r.routes)) return true;
  }
  return false;
}

export default function GraphqlBetaLink({ title }) {
  const { pathname } = useLocation();
  const [betaExists, setBetaExists] = useState(false);

  // Compute the candidate beta path by swapping alpha -> beta
  const betaPath = pathname.replace("/alpha/", "/beta/");

  useEffect(() => {
    const candidate = pathname.replace("/alpha/", "/beta/");

    if (candidate === pathname) {
      setBetaExists(false);
      return;
    }

    try {
      setBetaExists(routeExists(candidate, routes as any));
    } catch {
      setBetaExists(false);
    }
  }, [pathname]);

  return (
    <div className="bg-yellow-100 text-yellow-900 p-4 rounded mb-6 text-center border border-yellow-300">
      <p className="flex items-center justify-center mb-0">
        {betaExists ? (
          <>
            ⚠️ This is the <strong className="mx-2">alpha</strong> version of{" "}
            {title}.
            <a href={betaPath} className="underline text-yellow-800 ml-1">
              Switch to beta version →
            </a>
          </>
        ) : (
          <>
            ⚠️ This is the <strong className="mx-2">alpha</strong> version. The
            operation or type does not exist in the{" "}
            <a href="/references/sui-api/sui-graphql/beta/reference">
              beta version
            </a>
            .
          </>
        )}
      </p>
    </div>
  );
}
