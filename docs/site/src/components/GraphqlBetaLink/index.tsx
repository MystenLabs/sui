// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React, { useEffect, useState } from "react";
import { useLocation } from "@docusaurus/router";
import Link from "@docusaurus/Link";

export default function GraphqlBetaLink({ title }) {
  /*const { pathname } = useLocation();
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
  */
  return (
    <div className="bg-yellow-100 text-yellow-900 p-4 rounded mb-6 text-center border border-yellow-300">
      <>
        ⚠️ This is the <strong className="mx-1">beta</strong> version of the Sui
        GraphQL schema. The beta schema will replace the{" "}
        <Link href="/references/sui-api/sui-graphql/alpha/reference">
          alpha GraphQL schema
        </Link>{" "}
        upon its official release.
      </>
    </div>
  );
}
