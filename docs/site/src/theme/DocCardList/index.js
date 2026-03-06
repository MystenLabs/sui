// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0


import React from 'react';
import DocCardList from '@theme-original/DocCardList';
import { useCurrentSidebarCategory } from '@docusaurus/theme-common';

export default function DocCardListForCurrentSidebarCategory(props) {
  const scopeClass = 'docCardListScopeExclude';

  const css = `
    .${scopeClass} .col:has(a[href="/guides"]),
    .${scopeClass} .col:has(a[href="/guides/"]),
    .${scopeClass} .col:has(a[href="/concepts/"]),
    .${scopeClass} .col:has(a[href="/concepts"]),
    .${scopeClass} .col:has(a[href="/references/"]),
    .${scopeClass} .col:has(a[href="/references"]),
    .${scopeClass} .col:has(a[href="/standards/"]),
    .${scopeClass} .col:has(a[href="/standards"])
     {
      display: none !important;
    }
  `;

  try {
    const category = useCurrentSidebarCategory();
    return (
      <div className={scopeClass}>
        <style dangerouslySetInnerHTML={{ __html: css }} />
        <DocCardList items={category.items} />
      </div>
    );
  } catch {
    return (
      <div className={scopeClass}>
        <style dangerouslySetInnerHTML={{ __html: css }} />
        <DocCardList {...props} />
      </div>
    );
  }
}