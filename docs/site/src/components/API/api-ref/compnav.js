// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React from "react";
import Link from "@docusaurus/Link";

const CompNav = (props) => {
  const { json } = props;
  const schemas = json?.components?.schemas || {};

  return (
    <div className="mb-10 api-nav">
      <div className="api-nav-group">
        <div className="api-nav-title">Component schemas</div>

        {Object.keys(schemas).map((component) => (
          <Link
            key={component}
            href={`#${component.toLowerCase()}`}
            data-to-scrollspy-id={component.toLowerCase()}
            className="api-nav-link"
          >
            {component}
          </Link>
        ))}
      </div>
    </div>
  );
};

export default CompNav;
