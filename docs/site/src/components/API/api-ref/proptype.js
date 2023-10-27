// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React from "react";

import { getRef } from "..";

const PropType = (props) => {
  const { proptype } = props;

  const buildAnyof = (anyof) => {
    let a = [];
    anyof.forEach((el) => {
      if (typeof el["$ref"] !== "undefined") {
        a.push(getRef(el["$ref"]));
      } else if (typeof el.type !== "undefined") {
        a.push(el.type);
      }
    });
    return a.join(" | ");
  };

  let anyof = "";
  if (typeof proptype[1].anyOf !== "undefined") {
    anyof = buildAnyof(proptype[1].anyOf);
  }
  if (
    typeof proptype[1].additionalProperties !== "undefined" &&
    typeof proptype[1].additionalProperties.anyOf !== "undefined"
  ) {
    anyof = buildAnyof(proptype[1].additionalProperties.anyOf);
  }

  let allof = "";
  if (typeof proptype[1].allOf !== "undefined") {
    if (proptype[1].allOf.length == 1) {
      typeof proptype[1].allOf[0]["$ref"] !== "undefined"
        ? (allof = getRef(proptype[1].allOf[0]["$ref"]))
        : (allof = "SuiERR");
    }
  }

  let array = "";
  if (typeof proptype[1].type !== "undefined" && proptype[1].type === "array") {
    if (typeof proptype[1].items !== "undefined") {
      if (typeof proptype[1].items.type === "string") {
        array = "string";
      }
      if (typeof proptype[1].items.items !== "undefined") {
        let a = [];
        proptype[1].items.items.map((i) => {
          if (typeof i["$ref"] !== "undefined") {
            a.push(getRef(i["$ref"]));
          } else if (typeof i.type !== "undefined") {
            a.push(i.type);
          } else {
            a.push("SuiERR");
          }
        });
        array = a.join(", ");
      }
    }
  }

  return (
    <>
      {proptype[0]}

      {`<${proptype[1].type === "array" ? "[" : ""}
            ${
              proptype[1].items && proptype[1].items["$ref"]
                ? getRef(proptype[1].items["$ref"])
                : ""
            }
            ${array}
            ${proptype[1].type === "boolean" ? "Boolean" : ""}
            ${proptype[1].anyOf ? anyof : ""}
            ${proptype[1].type === "integer" ? proptype[1].format : ""}
            ${proptype[1].type === "string" ? proptype[1].type : ""}
            ${
              proptype[1].additionalProperties
                ? proptype[1].additionalProperties["$ref"]
                  ? getRef(proptype[1].additionalProperties["$ref"])
                  : "Boolean"
                : ""
            }
            ${proptype[1]["$ref"] ? getRef(proptype[1]["$ref"]) : ""}
            ${
              Array.isArray(proptype[1].type)
                ? `[${proptype[1].type.toString()}]`
                : ""
            }
            ${allof}
            ${proptype[1].type === "array" ? "]" : ""}>
      `}
    </>
  );
};

export default PropType;
