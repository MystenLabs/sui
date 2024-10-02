// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import React from "react";
import { MDXProvider } from "@mdx-js/react";
import MDXComponents from "@theme/MDXComponents";
import Tabs from "@theme/Tabs";
import TabItem from "@theme/TabItem";
import { Card, Cards } from "@site/src/components/Cards";
import EffortBox from "@site/src/components/EffortBox";
import BetaTag from "@site/src/components/BetaTag";
export default function MDXContent({ children }) {
  const suiComponents = {
    ...MDXComponents,
    Card,
    Cards,
    Tabs,
    TabItem,
    EffortBox,
    BetaTag,
  };
  return <MDXProvider components={suiComponents}>{children}</MDXProvider>;
}
