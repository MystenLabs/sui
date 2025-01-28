// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import React from "react";
import Nav from "./Nav";

export default function Protocol(props) {
  const { toc } = props;
  console.log(props);

  return (
    <p>
      <Nav toc={toc} />
    </p>
  );
}
