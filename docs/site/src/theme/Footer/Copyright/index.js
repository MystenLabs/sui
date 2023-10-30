// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import React from "react";
export default function FooterCopyright({ copyright }) {
  return (
    <div
<<<<<<< HEAD
      className="text-sm lg:text-xs xl:text-sm mt-2"
=======
      className="text-sm"
>>>>>>> main
      // Developer provided the HTML, so assume it's safe.
      // eslint-disable-next-line react/no-danger
      dangerouslySetInnerHTML={{ __html: copyright }}
    />
  );
}
