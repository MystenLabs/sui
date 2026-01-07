import * as React from "react";
import { useThemeConfig, ErrorCauseBoundary } from "@docusaurus/theme-common";
import {
  splitNavbarItems,
  useNavbarMobileSidebar,
} from "@docusaurus/theme-common/internal";
import NavbarItem from "@theme/NavbarItem";
import ThemeToggle from "@site/src/components/ThemeToggle";
// import SearchBar from "@theme/SearchBar";
import NavbarMobileSidebarToggle from "@theme/Navbar/MobileSidebar/Toggle";
import NavbarLogo from "@theme/Navbar/Logo";
import NavbarSearch from "@theme/Navbar/Search";
import GetStartedLink from "@site/src/components/GetStartedLink";
import SearchModal from "@site/src/components/Search/SearchModal";

function useNavbarItems() {
  // TODO temporary casting until ThemeConfig type is improved
  return useThemeConfig().navbar.items;
}

// Safe wrapper: if the provider isn't present, don't crash
function useMobileSidebarSafe() {
  try {
    return useNavbarMobileSidebar();
  } catch {
    // Emulate a "disabled" state so we can skip rendering the toggle safely.
    return { disabled: true, toggle: () => {} };
  }
}

function NavbarItems({ items }) {
  return (
    <div className="flex flex-[8_1_0%] items-center justify-start gap-8 min-[1100px]:gap-16">
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

function SearchLauncher() {
  const [open, setOpen] = React.useState(false);

  return (
    <>
      <button
        type="button"
        className="DocSearch DocSearch-Button flex items-center cursor-pointer"
        onClick={() => setOpen(true)}
      >
        <span className="DocSearch-Button-Container flex">
          <svg
            width="20"
            height="20"
            className="DocSearch-Search-Icon"
            viewBox="0 0 20 20"
            aria-hidden="true"
          >
            <path
              d="M14.386 14.386l4.0877 4.0877-4.0877-4.0877c-2.9418 2.9419-7.7115 2.9419-10.6533 0-2.9419-2.9418-2.9419-7.7115 0-10.6533 2.9418-2.9419 7.7115-2.9419 10.6533 0 2.9419 2.9418 2.9419 7.7115 0 10.6533z"
              stroke="currentColor"
              fill="none"
              fillRule="evenodd"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          </svg>
          <span className="DocSearch-Button-Placeholder font-semibold">
            Search
          </span>
        </span>
      </button>
      <SearchModal isOpen={open} onClose={() => setOpen(false)} />
    </>
  );
}

export default function NavbarContent() {
  const mobileSidebar = useMobileSidebarSafe();
  const items = useNavbarItems();
  const [leftItems, rightItems] = splitNavbarItems(items);
  const searchBarItem = items.find((item) => item.type === "search");

  React.useEffect(() => {
    try {
      window.initCookbook?.();
    } catch (e) {
      // Gracefully ignore errors if something goes wrong
      console.error("Error initializing Ask Cookbook", e);
    }
  }, []);

  return (
    <NavbarContentLayout
      left={
        <>
          {!mobileSidebar.disabled && <NavbarMobileSidebarToggle />}
          <NavbarLogo />
          <NavbarItems items={leftItems} />
        </>
      }
      right={
        <>
          <NavbarItems items={rightItems} />
          <ThemeToggle />
          {!searchBarItem && (
            <NavbarSearch>
              <SearchLauncher />
              <GetStartedLink />
            </NavbarSearch>
          )}
        </>
      }
    />
  );
}
