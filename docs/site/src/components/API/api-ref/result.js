// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React from "react";
import Markdown from "markdown-to-jsx";
import Ref from "./ref";
import PropType from "./proptype";
import { getRef } from "..";

const Result = (props) => {
  const { json, result } = props;
  const hasRef = typeof result.schema["$ref"] !== "undefined";

  let refObj = {};
  let ref = {};

  // ---- ORIGINAL LOGIC (unchanged) ----
  if (hasRef) {
    const schemaPath = getRef(result.schema["$ref"]);
    ref = json.components.schemas[schemaPath];
    if (ref.description) refObj.desc = ref.description;
    if (ref.required) refObj.reqs = ref.required;
    if (ref.properties && ref.properties.length > 0) {
      let x = 0;
      refObj.properties = [];
      try {
        for (const [k, v] of Object.entries(ref.properties)) {
          refObj.properties.push({
            name: k,
            type: null,
            desc: null,
            req: refObj.reqs.includes(k),
          });

          // (kept exactly as-is from your file)
          if (typeof v.type !== "undefined" && v.type == "array") {
            if (typeof v.items["$ref"] !== "undefined") {
              refObj.properties[x].type =
                "<[" +
                v.items["$ref"].substring(v.items["$ref"].lastIndexOf("/") + 1) +
                "]>";
            } else if (typeof v.items.type !== "undefined" && v.items.type === "integer") {
              refObj.properties[x].type = "<[" + v.items.format + "]>";
            } else if (typeof v.items.type !== "undefined" && v.items.type === "string") {
              refObj.properties[x].type = "<[" + v.items.type + "]>";
            } else if (typeof v.items.type !== "undefined" && v.items.type === "array") {
              let items = [];
              try {
                if (typeof v.items.items["$ref"] !== "undefined") {
                  items.push(`{${v.items.items["$ref"].substring(v.items.items["$ref"].lastIndexOf("/") + 1)}}`);
                } else if (v.items.items[0].type === "string") {
                  items.push("string");
                } else {
                  v.items.items.map((item) => {
                    if (typeof item["$ref"] !== "undefined") {
                      items.push(`{${item["$ref"].substring(item["$ref"].lastIndexOf("/") + 1)}}`);
                    } else if (typeof item.type !== "undefined") {
                      if (item.type === "integer") items.push(item.format);
                    }
                  });
                }
              } catch (err) {
                console.log(err);
                console.log(v);
              }
              refObj.properties[x].type = `<[${items.join(", ")}]>`;
            } else {
              console.log("Result not processed.");
              console.log(v);
            }
          } else if (typeof v.type !== "undefined" && v.type == "integer") {
            refObj.properties[x].type = "<" + v.format + ">";
          } else if (typeof v.allOf !== "undefined" && v.allOf.length == 1) {
            if (typeof v.allOf[0]["$ref"] !== "undefined") {
              refObj.properties[x].type =
                "<[" + v.allOf[0]["$ref"].substring(v.allOf[0]["$ref"].lastIndexOf("/") + 1) + "]>";
            } else {
              console.log("Error1");
            }
          } else if (typeof v.type !== "undefined" && v.type == "string") {
            refObj.properties[x].type = "<string>";
          } else if (typeof v["$ref"] !== "undefined") {
            refObj.properties[x].type =
              "<" + v["$ref"].substring(v["$ref"].lastIndexOf("/") + 1) + ">";
          } else if (typeof v.type !== "undefined" && v.type == "boolean") {
            refObj.properties[x].type = "<Boolean>";
          } else if (typeof v.anyOf !== "undefined") {
            if (typeof v.anyOf[0]["$ref"] !== "undefined") {
              refObj.properties[x].type =
                "<" + v.anyOf[0]["$ref"].substring(v.anyOf[0]["$ref"].lastIndexOf("/") + 1) + " | null>";
            } else {
              console.log("Error2");
            }
          } else if (typeof v.type !== "undefined" && v.type == "object") {
            if (typeof v.additionalProperties["$ref"] !== "undefined") {
              refObj.properties[x].type =
                "<" + v.additionalProperties["$ref"].substring(v.additionalProperties["$ref"].lastIndexOf("/") + 1) + ">";
            } else if (typeof v.additionalProperties.anyOf !== "undefined") {
              let type = [];
              v.additionalProperties.anyOf.map((prop) => {
                if (typeof prop["$ref"] !== "undefined") type.push(getRef(prop["$ref"]));
                else if (typeof prop.type !== "undefined") type.push(prop.type);
              });
              refObj.properties[x].type = `<${type.join(" | ")}>`;
            } else if (v.additionalProperties.type === "boolean") {
              refObj.properties[x].type = v.additionalProperties.type;
            } else {
              console.log("Error3");
              console.log(v);
            }
          } else if (typeof v.items !== "undefined" && v.items.type == "array") {
            if (typeof v.items.items[0]["$ref"] !== "undefined") {
              refObj.properties[x].type =
                "<[" +
                v.items.items[0]["$ref"].substring(v.items.items[0]["$ref"].lastIndexOf("/") + 1) +
                ", " +
                v.items.items[1].format +
                "]>";
            } else {
              console.log("Error4");
            }
          } else if (typeof v.type !== "undefined" && Array.isArray(v.type)) {
            if (v.type[0] == "string") refObj.properties[x].type = "<string, null>";
            else if (v.type[0] == "integer") refObj.properties[x].type = "<" + v.format + ", null>";
          } else if (v.description) {
            refObj.properties[x].desc = v.description;
          } else {
            console.log("A Result was not processed:\n");
            console.log(v);
          }

          x++;
        }
      } catch (err) {
        console.log(err);
      }
    }
  }
  // ---- END ORIGINAL LOGIC ----

  const hasRefProps = refObj.properties && refObj.properties.length > 0;

  return (
    <div className="api-card api-card-pad">
      <div className="api-section-title">Result</div>

      <div className="mb-3">
        <PropType proptype={[result.name, result.schema]} />
      </div>

      {refObj.desc && !hasRef && (
        <div className="api-muted">
          <Markdown>{refObj.desc}</Markdown>
        </div>
      )}

      {hasRef && <Ref schema={ref} />}

      {hasRef && hasRefProps && (
        <div className="api-section">
          <div className="api-section-title">Properties</div>

          <div className="api-row-head">
            <div>Name &amp; Type</div>
            <div>Required</div>
            <div>Description</div>
          </div>

          <div className="api-rows">
            {refObj.properties.map((p) => (
              <div key={p.name} className="api-row">
                <div className="api-cell api-cell-scroll">
                  <span className="inline-flex items-center gap-2 min-w-0">
                    <span className="font-semibold">{p.name}</span>
                    {p.type ? <span className="api-typechip">{p.type}</span> : null}
                  </span>
                </div>
                <div className="api-cell">
                  <span className={p.req ? "api-badge-yes" : "api-badge-no"}>
                    {p.req ? "Required" : "Optional"}
                  </span>
                </div>
                <div className="api-cell api-cell-scroll">
                  {p.desc ? <Markdown>{p.desc}</Markdown> : <span className="api-muted">—</span>}
                </div>
              </div>
            ))}
          </div>
        </div>
      )}

      {!result && <div className="api-muted">Not applicable</div>}
    </div>
  );
};

export default Result;
