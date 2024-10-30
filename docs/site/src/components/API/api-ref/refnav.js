// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React from "react";
import Link from "@docusaurus/Link";
import NetworkSelect from "./networkselect";

const RefNav = (props) => {
  const { json, apis } = props;

  return (
    <div className="mb-8">
      <div className="sticky -top-12 -mt-8 pt-8 pb-2 bg-white dark:bg-ifm-background-color-dark">
        <NetworkSelect />
      </div>

      {apis.map((api) => {
        return (
          <div key={`${api.replaceAll(/\s/g, "-").toLowerCase()}`}>
            <Link
              href={`#${api.replaceAll(/\s/g, "-").toLowerCase()}`}
              data-to-scrollspy-id={`${api
                .replaceAll(/\s/g, "-")
                .toLowerCase()}`}
              className="hover:no-underline pt-4 block text-black dark:text-white hover:text-sui-blue dark:hover:text-sui-blue"
            >
              {api}
            </Link>
            {json["methods"]
              .filter((method) => method.tags[0].name == api)
              .map((method) => {
                return (
                  <Link
                    className="my-1 pl-4 block text-sui-gray-95 dark:text-sui-grey-35 hover:no-underline dark:hover:text-sui-blue"
                    key={`link-${method.name.toLowerCase()}`}
                    href={`#${method.name.toLowerCase()}`}
                    data-to-scrollspy-id={`${method.name.toLowerCase()}`}
                  >
                    {method.name}
                  </Link>
                );
              })}
              
          </div>
        );
      })}
    </div>
  );
};

export default RefNav;
