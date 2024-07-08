// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import React from "react";
import Link from "@docusaurus/Link";
import { useThemeConfig, ErrorCauseBoundary } from "@docusaurus/theme-common";
import {
  splitNavbarItems,
  useNavbarMobileSidebar,
} from "@docusaurus/theme-common/internal";
import NavbarItem from "@theme/NavbarItem";
import NavbarColorModeToggle from "@theme/Navbar/ColorModeToggle";
import SearchBar from "@theme/SearchBar";
import NavbarMobileSidebarToggle from "@theme/Navbar/MobileSidebar/Toggle";
import NavbarLogo from "@theme/Navbar/Logo";
import NavbarSearch from "@theme/Navbar/Search";
import styles from "./styles.module.css";
import { useLocation } from "@docusaurus/router";

function useNavbarItems() {
  // TODO temporary casting until ThemeConfig type is improved
  return useThemeConfig().navbar.items;
}
function NavbarItems({ items }) {
  return (
    <div className={styles.navbar__items_wrapper}>
      {items.map((item, i) => (
        <ErrorCauseBoundary
          key={i}
          onError={(error) =>
            new Error(
              `A theme navbar item failed to render.
Please double-check the following navbar item (themeConfig.navbar.items) of your Docusaurus config:
${JSON.stringify(item, null, 2)}`,
              { cause: error },
            )
          }
        >
          <NavbarItem {...item} />
        </ErrorCauseBoundary>
      ))}
    </div>
  );
}
function NavbarContentLayout({ left, right }) {
  return (
    <div className="navbar__inner">
      <div className="navbar__items">{left}</div>
      <div className="navbar__items navbar__items--right">{right}</div>
    </div>
  );
}
export default function NavbarContent() {
  const mobileSidebar = useNavbarMobileSidebar();
  const items = useNavbarItems();
  const location = useLocation();
  const [leftItems, rightItems] = splitNavbarItems(items);
  const searchBarItem = items.find((item) => item.type === "search");
  const isHomePage = location.pathname === "/";

  return (
    <NavbarContentLayout
      left={
        // TODO stop hardcoding items?
        <>
          {!mobileSidebar.disabled && <NavbarMobileSidebarToggle />}
          <NavbarLogo />
          <NavbarItems items={leftItems} />
        </>
      }
      right={
        // TODO stop hardcoding items?
        // Ask the user to add the respective navbar items => more flexible
        <>
          <NavbarItems items={rightItems} />
          {!isHomePage && (
            <NavbarColorModeToggle className={styles.colorModeToggle} />
          )}
          {!searchBarItem && (
            <NavbarSearch>
              <SearchBar />
              {isHomePage && (
                <Link
                  to="/guides#get-started-developing-on-sui"
                  className="button-cta"
                >
                  Get started
                </Link>
              )}
            </NavbarSearch>
          )}
        </>
      }
    />
  );
}
