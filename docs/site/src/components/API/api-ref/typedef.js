import React, { useState } from "react";
import _ from "lodash";
import { getRef } from "../index";

import { Light as SyntaxHighlighter } from 'react-syntax-highlighter';
import json from 'react-syntax-highlighter/dist/esm/languages/hljs/json';
import docco from 'react-syntax-highlighter/dist/esm/styles/hljs/docco';
import dark from 'react-syntax-highlighter/dist/esm/styles/hljs/dracula';

const TypeDef = (props) => {

  const { schema, schemas } = props;
  const schemaObj = schemas[schema];
  let refs = [{ title: schema, ...schemaObj }];

  const collectRefs = (obj) => {
    for (const [key, value] of Object.entries(obj)) {
      if (value && Array.isArray(value)) {
        for (let x = 0; x < value.length; x++) {
          collectRefs(value[x]);
        }
      } else if (value && typeof value === "object") {
        collectRefs(value);
      }
      if (key == "$ref")
            refs.push({ title: getRef(value), ...schemas[getRef(value)] });
    }
  };

  collectRefs(schemaObj);

  refs.map( (ref) => {
        collectRefs(schemas[ref.title]);
  })

  refs.map( (ref) => {
    collectRefs(schemas[ref.title]);
})

  return (
    <div>
      {_.uniqWith(refs, (a, b) => {return a.title === b.title}).map((curObj, idx) => {
        return (
          <div key={idx}>
            <p className="text-lg font-bold mb-0 mt-8">{curObj.title}</p>
            <hr className="mt-0" />
            <SyntaxHighlighter language={json} style={localStorage.theme == "light" ? docco : dark}>{JSON.stringify(_.omit(curObj, "title"), null, 4)}</SyntaxHighlighter>
          </div>
        );
      })}
    </div>
  );
};

export default TypeDef;
