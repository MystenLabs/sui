// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React, { useState, useEffect } from "react";
import {Highlight, themes} from "prism-react-renderer";

import axios from "axios";
import { Prism } from "prism-react-renderer";
import CopyButton from "@theme/CodeBlock/CopyButton";
import { Language } from "prism-react-renderer";

import styles from "./styles.module.css";
require("prismjs/components/prism-rust");

type LangExt = Language | "rust";

const BASE = "https://raw.githubusercontent.com/MystenLabs/sui/main";

export default function ExampleImport(props) {
  const [example, setExample] = useState(null);
  const {
    file,
    type,
    lineStart,
    lineEnd,
    showLineNumbers,
    appendToCode,
    prependToCode,
  } = props;
  const fileUrl = BASE + file;
  const fileExt = file.split(".").pop();
  const prefix = file.replaceAll("/", "_").replaceAll(".", "_").toLowerCase();
  const subStart = lineStart - 1 || 0;
  const subEnd = lineEnd || 0;

  let highlight: LangExt = fileExt; //default

  if (type == "move" || highlight == ("move" as LangExt)) {
    highlight = "rust"; //move is not an option
  } else if (typeof type != "undefined") {
    highlight = type;
  }

  useEffect(() => {
    axios
      .get(fileUrl)
      .then((response) => {
        if (subStart > 0 || subEnd > 0) {
          let appends = [];
          let lines = response.data.split("\n");
          if (subStart > 0 && subEnd > 0) {
            lines.splice(0, subStart);
            lines.splice(subEnd - subStart, lines.length);
          } else if (subStart > 0) {
            lines.splice(0, subStart);
          } else if (subEnd > 0) {
            lines.splice(subEnd, lines.length);
          }
          if (appendToCode) {
            appends = appendToCode.split("\n");
            lines = [...lines, ...appends];
          }
          if (prependToCode) {
            appends = prependToCode.split("\n");
            lines = [...appends, ...lines];
          }
          setExample(lines.join("\n"));
        } else {
          setExample(response.data);
        }
      })
      .catch((error) => {
        setExample("Error loading file.");
        console.log(error);
      });
  }, []);

  if (!example) return null;

  return (
    <Highlight
      Prism={Prism}
      code={example}
      language={highlight as Language}
      theme={themes.github}
    >
      {({ className, style, tokens, getLineProps, getTokenProps }) => (
        <div className="codeBlockContainer_node_modules-@docusaurus-theme-classic-lib-theme-CodeBlock-Container-styles-module theme-code-block">
          <div className="codeBlockContent_node_modules-@docusaurus-theme-classic-lib-theme-CodeBlock-Content-styles-module">
            <pre>
              {tokens.map((line, i) => (
                <div key={"div_" + prefix + i} {...getLineProps({ line })}>
                  {showLineNumbers && (
                    <span className={styles.lineNumbers}>
                      {i + 1}
                      {"\t"}
                    </span>
                  )}
                  {line.map((token, key) => (
                    <span key={prefix + key} {...getTokenProps({ token })} />
                  ))}
                </div>
              ))}
            </pre>
            <div className="buttonGroup_node_modules-@docusaurus-theme-classic-lib-theme-CodeBlock-Content-styles-module">
              <CopyButton code={example} />
            </div>
          </div>
        </div>
      )}
    </Highlight>
  );
}
