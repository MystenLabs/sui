// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import React from "react";
import NavbarLayout from "@theme/Navbar/Layout";
import NavbarContent from "@theme/Navbar/Content";
import {
  NavbarProvider,
  ScrollControllerProvider,
  ColorModeProvider,
} from "@docusaurus/theme-common/internal";
export default function Navbar() {
  return (
    <NavbarProvider>
      <ScrollControllerProvider>
        <ColorModeProvider>
          <NavbarLayout>
            <NavbarContent />
          </NavbarLayout>
        </ColorModeProvider>
      </ScrollControllerProvider>
    </NavbarProvider>
  );
}
