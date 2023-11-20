// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React, { useState, useEffect } from "react";
import Markdown from "markdown-to-jsx";
import { Light as SyntaxHighlighter } from "react-syntax-highlighter";
import js from "react-syntax-highlighter/dist/esm/languages/hljs/json";
import docco from "react-syntax-highlighter/dist/esm/styles/hljs/docco";
import dark from "react-syntax-highlighter/dist/esm/styles/hljs/dracula";

SyntaxHighlighter.registerLanguage("json", js);

const Examples = (props) => {
  const [light, setLight] = useState(true);

  useEffect(() => {
    const checkTheme = () => {
      const theme = localStorage.getItem("theme");

      if (theme !== "light") {
        setLight(false);
      }
    };

    window.addEventListener("storage", checkTheme);

    return () => {
      window.removeEventListener("storage", checkTheme);
    };
  }, [light]);

  const { method, examples } = props;

  const request = {
    jsonrpc: "2.0",
    id: 1,
    method,
    params: [],
  };

  let keyedParams = examples[0].params;

  keyedParams.forEach((item) => {
    request.params.push(item.value);
  });

  let stringRequest = JSON.stringify(request, null, 2);
  stringRequest = stringRequest.replaceAll('"  value": ', "");

  const response = {
    jsonrpc: "2.0",
    result: {},
    id: 1,
  };

  response.result = examples[0].result.value;

  return (
    <div className="mx-4">
      <p className="my-2">
        <Markdown>{examples[0].name}</Markdown>
      </p>
      {examples[0].params && (
        <div>
          <p className="font-bold mt-4 text-sui-gray-80 dark:text-sui-gray-50">
            Request
          </p>
          <pre className="p-2 pb-0 max-h-96	dark:bg-sui-ghost-dark bg-sui-ghost-white rounded-lg mt-4 overflow-x-auto border dark:border-sui-gray-75">
            <code className="text-base">
              <SyntaxHighlighter language={js} style={light ? docco : dark}>
                {stringRequest}
              </SyntaxHighlighter>
            </code>
          </pre>
        </div>
      )}
      {examples[0].result.value && (
        <div>
          <p className="font-bold mt-6 text-sui-gray-80 dark:text-sui-gray-50">
            Response
          </p>
          <pre className="p-2 pb-0 max-h-96 dark:bg-sui-ghost-dark bg-sui-ghost-white rounded-lg mt-4 overflow-x-auto border dark:border-sui-gray-75">
            <code className="text-base">
              <SyntaxHighlighter language={js} style={light ? docco : dark}>
                {JSON.stringify(response, null, 2)}
              </SyntaxHighlighter>
            </code>
          </pre>
        </div>
      )}
    </div>
  );
};

export default Examples;
