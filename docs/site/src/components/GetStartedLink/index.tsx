// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React from "react";
import Link from "@docusaurus/Link";
import { useLocation } from "@docusaurus/router";

export default function GetStartedLink() {
  const location = useLocation();
  return (
    <>
      {location.pathname === "/" && (
        <Link to="/guides#get-started-developing-on-sui" className="button-cta">
          Get started
        </Link>
      )}
    </>
  );
}
