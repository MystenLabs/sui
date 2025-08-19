// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
// 
// This code ensures <Tabs> <TabItems> are in the MDX scope globally

import React from 'react';
import MDXComponents from '@theme-original/MDXComponents';
import CodeFromFile from '@site/src/components/CodeFromFile';


import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

export default {
  ...MDXComponents,
  Tabs,
  TabItem,
  CodeFromFile,
};
