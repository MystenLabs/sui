// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import React from "react";
import { isMultiColumnFooterLinks } from "@docusaurus/theme-common";
import FooterLinksMultiColumn from "@theme/Footer/Links/MultiColumn";
import FooterLinksSimple from "@theme/Footer/Links/Simple";
export default function FooterLinks({ links }) {
  return isMultiColumnFooterLinks(links) ? (
    <FooterLinksMultiColumn columns={links} />
  ) : (
    <FooterLinksSimple links={links} />
  );
}
