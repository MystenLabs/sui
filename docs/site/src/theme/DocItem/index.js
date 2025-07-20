// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React from "react";
import DocItem from "@theme-original/DocItem";
import GraphqlBetaLink from "@site/src/components/GraphqlBetaLink";

export default function DocItemWrapper(props) {
  const isGraphQlAlpha = props.content.frontMatter?.isGraphQlAlpha;
  const title = props.content.frontMatter?.title || "GraphQL";
  return (
    <>
      {isGraphQlAlpha && <GraphqlBetaLink title={title}></GraphqlBetaLink>}
      <DocItem {...props} />
    </>
  );
}
