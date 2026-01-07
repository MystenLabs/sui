/*
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
*/
import React from "react";
import MDXComponentsOriginal from "@theme-original/MDXComponents";
import Tabs from "@theme/Tabs";
import TabItem from "@theme/TabItem";
import { Card, Cards } from "@site/src/shared/components/Cards";
import CodeBlock from "@theme/CodeBlock";
import DocCardList from "@theme/DocCardList";
import BrowserOnly from "@docusaurus/BrowserOnly";
import UnsafeLink from "@site/src/shared/components/UnsafeLink";
import RelatedLink from "@site/src/shared/components/RelatedLink";
import ImportContent from "@site/src/shared/components/ImportContent";

// Site-specific components - these may need to stay in each site
// import EffortBox from "@site/src/components/EffortBox";
// import BetaTag from "@site/src/components/BetaTag";
// import ProtocolConfig from "@site/src/components/ProtocolConfig";

export default {
  ...MDXComponentsOriginal,
  Card,
  Cards,
  Tabs,
  TabItem,
  CodeBlock,
  DocCardList,
  BrowserOnly,
  UnsafeLink,
  RelatedLink,
  ImportContent,
  // EffortBox,
  // BetaTag,
  // ProtocolConfig,
};