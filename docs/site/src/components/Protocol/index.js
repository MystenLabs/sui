// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import React from "react";


export default function Protocol({ modules: { jsonData } }) {
  const jsonContent = require(jsonData);
  console.log(jsonContent);
  return <></>;
}
