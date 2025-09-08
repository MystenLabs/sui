// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
//
// This code ensures <Tabs> <TabItems> are in the MDX scope globally

import React from "react";
import MDXComponents from "@theme-original/MDXComponents";

export default {
  ...MDXComponents,
};
