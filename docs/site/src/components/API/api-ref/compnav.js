// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React from "react";
import Link from "@docusaurus/Link";
import NetworkSelect from "./networkselect";

const CompNav = (props) => {
  const { json, apis } = props;

  return (
    <div>
          <div>
          <h2>Component schemas</h2>
          {Object.keys(json["components"]["schemas"]).map((component) => {
            return (
            <div key={component}>
              <Link href={`#${component.toLowerCase()}`}
              data-to-scrollspy-id={`${component.toLowerCase()}`}
              className="hover:no-underline pt-4 block text-black dark:text-white hover:text-sui-blue dark:hover:text-sui-blue">
                {component}
              </Link>
              </div>
          )})}
          </div>      
    </div>
  );
};

export default CompNav;
