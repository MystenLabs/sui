// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import React from "react";
import { useDocusaurusContext } from "@docusaurus/core";

export default function Protocol({ modules: { jsonData } }) {
  const jsonContent = require(jsonData);

  return (
    <div>
      <h1>{jsonContent.title}</h1>
      <p>{jsonContent.content}</p>
    </div>
  );
}
