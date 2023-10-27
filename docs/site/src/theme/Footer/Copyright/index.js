// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import React from "react";
export default function FooterCopyright({ copyright }) {
  return (
    <div
      className="text-sm"
      // Developer provided the HTML, so assume it's safe.
      // eslint-disable-next-line react/no-danger
      dangerouslySetInnerHTML={{ __html: copyright }}
    />
  );
}
