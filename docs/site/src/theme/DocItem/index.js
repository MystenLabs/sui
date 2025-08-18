// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React from "react";
import DocItem from "@theme-original/DocItem";
import GraphqlBetaLink from "@site/src/components/GraphqlBetaLink";

export default function DocItemWrapper(props) {
  const isGraphQlBeta = props.content.frontMatter?.isGraphQlBeta;
  const title = props.content.frontMatter?.title || "GraphQL";
  return (
    <>
      {isGraphQlBeta && <GraphqlBetaLink title={title}></GraphqlBetaLink>}
      <DocItem {...props} />
    </>
  );
}
