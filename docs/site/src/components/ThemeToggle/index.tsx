// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React from "react";
import NavbarColorModeToggle from "@theme/Navbar/ColorModeToggle";
import { useLocation } from "@docusaurus/router";

export default function ThemeToggle() {
  const location = useLocation();
  return (
    <div className="theme-toggle-wrapper text-white max-[996px]:hidden">
      {location.pathname !== "/" && <NavbarColorModeToggle />}
    </div>
  );
}
