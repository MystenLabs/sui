// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React, { useEffect, useState } from "react";
import { useLocation } from "@docusaurus/router";

export default function GraphqlBetaLink({ title }) {
  const { pathname } = useLocation();
  const [betaExists, setBetaExists] = useState(false);

  const betaPath = pathname.replace("/alpha/", "/beta/");

  useEffect(() => {
    if (pathname.includes("/alpha/")) {
      fetch(betaPath, { method: "HEAD" })
        .then((res) => {
          if (res.ok) setBetaExists(true);
        })
        .catch(() => {});
    }
  }, [pathname, betaPath]);

  return (
    <div className="bg-yellow-100 text-yellow-900 p-4 rounded mb-6 text-center border border-yellow-300">
      <p class="flex items-center justify-center mb-0">
        {betaExists ? (
          <>
            ⚠️ This is the <strong className="mx-1">alpha</strong> version of {title}.
            <a href={betaPath} className="underline text-yellow-800 ml-1">
              Switch to beta version →
            </a>
          </>
        ) : (
          <>
            ⚠️ This is the <strong>alpha</strong> version. The operation or type
            does not exist in the beta version. Use the left navigation to
            browse the GraphQL Beta information.
          </>
        )}
      </p>
    </div>
  );
}
