// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0


import React from 'react';
import DocCardList from '@theme-original/DocCardList';
import { useCurrentSidebarCategory } from '@docusaurus/theme-common';

export default function DocCardListForCurrentSidebarCategory(props) {
  try {
    const category = useCurrentSidebarCategory();
    return (
      <div className="docCardListScopeExclude">
        <DocCardList items={category.items} />
      </div>
    );
  } catch {
    return (
      <div className="docCardListScopeExclude">
        <DocCardList {...props} />
      </div>
    );
  }
}
