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
      setLight(theme === "light");
    };
    window.addEventListener("storage", checkTheme);
    return () => window.removeEventListener("storage", checkTheme);
  }, []);

  const { method, examples } = props;

  const request = { jsonrpc: "2.0", id: 1, method, params: [] };
  const keyedParams = examples[0].params ?? [];
  keyedParams.forEach((item) => request.params.push(item.value));

  let stringRequest = JSON.stringify(request, null, 2).replaceAll('"  value": ', "");

  const response = { jsonrpc: "2.0", result: examples[0]?.result?.value ?? {}, id: 1 };

  return (
    <div className="api-card api-card-pad">
      <div className="api-section-title">Example</div>

      {examples?.[0]?.name && (
        <div className="api-muted">
          <Markdown>{examples[0].name}</Markdown>
        </div>
      )}

      {examples[0].params && (
        <div className="api-section">
          <div className="api-code">
            <div className="api-code-title">Request</div>
            <div className="api-code-body">
              <SyntaxHighlighter language={js} style={light ? docco : dark}>
                {stringRequest}
              </SyntaxHighlighter>
            </div>
          </div>
        </div>
      )}

      {examples?.[0]?.result?.value && (
        <div className="api-section">
          <div className="api-code">
            <div className="api-code-title">Response</div>
            <div className="api-code-body">
              <SyntaxHighlighter language={js} style={light ? docco : dark}>
                {JSON.stringify(response, null, 2)}
              </SyntaxHighlighter>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};

export default Examples;
