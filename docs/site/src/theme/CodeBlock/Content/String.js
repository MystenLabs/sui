// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Revised to use the built-in Docusaurus feature
// rather than use custom code

import * as React from 'react';
import OriginalString from '@theme-original/CodeBlock/Content/String';

export default function StringContent(props) {
  return <OriginalString {...props} />;
}
