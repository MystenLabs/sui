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
import CodeFromFile from '@site/src/components/CodeFromFile';
import CodeBlock from '@theme/CodeBlock';
import DocCardList from '@theme/DocCardList';
import ProtocolConfig from "@site/src/components/ProtocolConfig";
import YTCarousel from "@site/src/components/YTCarousel";
import BrowserOnly from '@docusaurus/BrowserOnly';


export default function MDXContent({ children }) {
  const suiComponents = {
    ...MDXComponents,
    Card,
    Cards,
    Tabs,
    TabItem,
    EffortBox,
    BetaTag,
    CodeFromFile,
    CodeBlock,
    DocCardList,
    ProtocolConfig,
    YTCarousel,
    BrowserOnly,
  };
  return <MDXProvider components={suiComponents}>{children}</MDXProvider>;
}
