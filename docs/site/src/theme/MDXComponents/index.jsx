// This code ensures <Tabs> <TabItems> are in the MDX scope globally

import React from 'react';
import MDXComponents from '@theme-original/MDXComponents';


import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

export default {
  ...MDXComponents,
  Tabs,
  TabItem,
};
