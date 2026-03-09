// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React from "react";
import _ from "lodash";
import { getRef } from "../index";

import { Light as SyntaxHighlighter } from "react-syntax-highlighter";
import json from "react-syntax-highlighter/dist/esm/languages/hljs/json";
import dark from "react-syntax-highlighter/dist/esm/styles/hljs/dracula";

const TypeDef = (props) => {
  const { schema, schemas } = props;
  const schemaObj = schemas[schema];
  let refs = [{ title: schema, ...schemaObj }];

  const collectRefs = (obj) => {
    for (const [key, value] of Object.entries(obj)) {
      if (value && Array.isArray(value)) {
        value.forEach((v) => collectRefs(v));
      } else if (value && typeof value === "object") {
        collectRefs(value);
      }
      if (key === "$ref") refs.push({ title: getRef(value), ...schemas[getRef(value)] });
    }
  };

  collectRefs(schemaObj);
  refs.forEach((ref) => collectRefs(schemas[ref.title]));
  refs.forEach((ref) => collectRefs(schemas[ref.title]));

  return (
    <div className="api-card api-card-pad">
      <div className="api-section-title">Type definitions</div>

      {_.uniqWith(refs, (a, b) => a.title === b.title).map((curObj, idx) => (
        <div key={idx} className="mt-4">
          <div className="flex items-center justify-between gap-2">
            <div className="font-extrabold">{curObj.title}</div>
            <span className="api-chip">JSON</span>
          </div>

          <div className="api-code mt-2">
            <div className="api-code-body">
              <SyntaxHighlighter language={json} style={dark}>
                {JSON.stringify(_.omit(curObj, "title"), null, 4)}
              </SyntaxHighlighter>
            </div>
          </div>
        </div>
      ))}
    </div>
  );
};

export default TypeDef;
