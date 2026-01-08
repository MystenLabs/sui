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

// Debug: Log what each import actually is
console.log("[MDXComponents Debug]");
console.log("Tabs:", typeof Tabs, Tabs);
console.log("TabItem:", typeof TabItem, TabItem);
console.log("Card:", typeof Card, Card);
console.log("Cards:", typeof Cards, Cards);
console.log("CodeBlock:", typeof CodeBlock, CodeBlock);
console.log("DocCardList:", typeof DocCardList, DocCardList);
console.log("BrowserOnly:", typeof BrowserOnly, BrowserOnly);
console.log("UnsafeLink:", typeof UnsafeLink, UnsafeLink);
console.log("RelatedLink:", typeof RelatedLink, RelatedLink);
console.log("ImportContent:", typeof ImportContent, ImportContent);

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
};