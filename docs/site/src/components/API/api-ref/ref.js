// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React from "react";
import Markdown from "markdown-to-jsx";
import PropType from "./proptype";

const Ref = (props) => {
  const { schema } = props;
  const requireds =
    typeof schema.required !== "undefined" ? schema.required : [];

  return (
    <div>
      <p>{schema.description && <Markdown>{schema.description}</Markdown>}</p>
      {schema.properties && (
        <>
          <div className="grid grid-cols-6 ml-4 bg-sui-gray-50 dark:bg-sui-gray-80 rounded-lg">
            <div className="col-span-2 p-2 text-sui-gray-95 dark:text-sui-gray-50 font-bold">
              Parameter
            </div>
            <div className="p-2 text-sui-gray-95 dark:text-sui-gray-50 text-sui-gray-35 font-bold">
              Required
            </div>
            <div className="col-span-3 p-2 text-sui-gray-95 dark:text-sui-gray-50 text-sui-gray-35 font-bold">
              Description
            </div>
          </div>

          {Object.entries(schema.properties).map((property, idx) => {
            return (
              <div
                key={idx}
                className="grid grid-cols-6 even:bg-sui-gray-35 dark:even:bg-sui-gray-95 ml-4 rounded-lg items-center"
              >
                <div className="col-span-2 p-2 overflow-x-auto">
                  <PropType proptype={property}></PropType>
                </div>
                <div className="flex items-center ml-2">
                  {requireds.includes(property[0]) ? "Yes" : "No"}
                </div>
                <div className="col-span-3 p-2 overflow-x-auto">
                  {property[1].description && (
                    <Markdown>{property[1].description}</Markdown>
                  )}
                </div>
              </div>
            );
          })}
        </>
      )}
    </div>
  );
};

export default Ref;
