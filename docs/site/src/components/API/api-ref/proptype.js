// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React from "react";
import { getRef } from "..";

const PropType = (props) => {
  const { proptype } = props;

  const buildAnyof = (anyof) =>
    anyof
      .map((el) => (typeof el["$ref"] !== "undefined" ? getRef(el["$ref"]) : el.type))
      .filter(Boolean)
      .join(" | ");

  let anyof = "";
  if (typeof proptype[1].anyOf !== "undefined") anyof = buildAnyof(proptype[1].anyOf);
  if (
    typeof proptype[1].additionalProperties !== "undefined" &&
    typeof proptype[1].additionalProperties.anyOf !== "undefined"
  ) {
    anyof = buildAnyof(proptype[1].additionalProperties.anyOf);
  }

  let allof = "";
  if (typeof proptype[1].allOf !== "undefined" && proptype[1].allOf.length === 1) {
    allof = proptype[1].allOf[0]?.["$ref"] ? getRef(proptype[1].allOf[0]["$ref"]) : "SuiERR";
  }

  let array = "";
  if (proptype[1].type === "array" && typeof proptype[1].items !== "undefined") {
    const items = proptype[1].items;
    if (items?.items) {
      const a = items.items
        .map((i) => (i["$ref"] ? getRef(i["$ref"]) : i.type ?? "SuiERR"))
        .filter(Boolean);
      array = a.join(", ");
    } else if (items?.type === "string") {
      array = "string";
    }
  }

  const typeString = `<${
    proptype[1].type === "array" ? "[" : ""
  }${proptype[1].items?.["$ref"] ? getRef(proptype[1].items["$ref"]) : ""}${array}${
    proptype[1].type === "boolean" ? "Boolean" : ""
  }${proptype[1].anyOf ? anyof : ""}${proptype[1].type === "integer" ? proptype[1].format : ""}${
    proptype[1].type === "string" ? "string" : ""
  }${
    proptype[1].additionalProperties
      ? proptype[1].additionalProperties["$ref"]
        ? getRef(proptype[1].additionalProperties["$ref"])
        : "Boolean"
      : ""
  }${proptype[1]["$ref"] ? getRef(proptype[1]["$ref"]) : ""}${
    Array.isArray(proptype[1].type) ? `[${proptype[1].type.toString()}]` : ""
  }${allof}${proptype[1].type === "array" ? "]" : ""}>`;

  return (
    <span className="inline-flex items-center gap-2 min-w-0">
      <span className="font-semibold">{proptype[0]}</span>
      <span className="api-typechip">{typeString.replace(/\s+/g, " ").trim()}</span>
    </span>
  );
};

export default PropType;
