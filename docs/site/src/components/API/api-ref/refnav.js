// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React from "react";
import Link from "@docusaurus/Link";
import NetworkSelect from "./networkselect";

const RefNav = (props) => {
  const { json, apis } = props;

  return (
    <div className="mb-8 api-nav">
      <div className="sticky -top-12 -mt-8 pt-8 pb-2 bg-white dark:bg-ifm-background-color-dark">
        <div className="api-card api-card-pad">
          <NetworkSelect />
        </div>
      </div>

      {apis.map((api) => {
        const apiId = api.replaceAll(/\s/g, "-").toLowerCase();
        return (
          <div key={apiId} className="api-nav-group">
            <Link
              href={`#${apiId}`}
              data-to-scrollspy-id={apiId}
              className="api-nav-link api-nav-title"
            >
              {api}
            </Link>

            {json["methods"]
              .filter((method) => method.tags[0].name === api)
              .map((method) => (
                <Link
                  key={`link-${method.name.toLowerCase()}`}
                  href={`#${method.name.toLowerCase()}`}
                  data-to-scrollspy-id={method.name.toLowerCase()}
                  className="api-nav-link"
                >
                  {method.name}
                </Link>
              ))}
          </div>
        );
      })}
    </div>
  );
};

export default RefNav;
