
//
// This code ensures that <Tabs> and <TabItems> are formatted correctly in
// code that is inserted into a page with {@include: ...}
// plus any pages that require mdx: format md in the frontmatter
// to support the {@include: ...} behavior.

import ExecutionEnvironment from "@docusaurus/ExecutionEnvironment";

const inited = new WeakSet();

function getTabs(container) {
  const list = container.querySelector('ul.tabs[role="tablist"]');
  const tabs = list ? Array.from(list.querySelectorAll('[role="tab"]')) : [];
  const panels = Array.from(
    container.querySelectorAll('.margin-top--md > [role="tabpanel"]'),
  );
  return { list, tabs, panels };
}

function activate(container, value) {
  const { tabs, panels } = getTabs(container);
  tabs.forEach((t) => {
    const active = t.dataset.value === value;
    t.classList.toggle("tabs__item--active", active);
    t.setAttribute("aria-selected", active ? "true" : "false");
    t.tabIndex = active ? 0 : -1;
  });
  panels.forEach((p) =>
    p.dataset.value === value
      ? p.removeAttribute("hidden")
      : p.setAttribute("hidden", ""),
  );
}

function initContainer(container) {
  if (inited.has(container)) return;
  const { tabs } = getTabs(container);
  if (!tabs.length) return;

  const querySync = container.hasAttribute("data-querystring");
  const group = container.getAttribute("data-group");
  const qs = querySync
    ? new URL(window.location.href).searchParams.get(group || "tab")
    : null;
  const initial =
    qs ??
    (tabs.find((t) => t.classList.contains("tabs__item--active"))?.dataset
      .value ||
      tabs[0].dataset.value);

  activate(container, initial);
  inited.add(container);
}

function writeQuery(container, value) {
  if (!container.hasAttribute("data-querystring")) return;
  const group = container.getAttribute("data-group") || "tab";
  const url = new URL(window.location.href);
  url.searchParams.set(group, value);
  history.replaceState(null, "", url.toString());
}

function setAllInGroup(groupId, value) {
  document
    .querySelectorAll(`.tabs-container[data-group="${groupId}"]`)
    .forEach((c) => activate(c, value));
}

function bindDelegatedHandlers() {
  document.addEventListener("click", (e) => {
    const tab = e.target.closest('.tabs-container [role="tab"]');
    if (!tab) return;
    const container = tab.closest(".tabs-container");
    initContainer(container);
    const value = tab.dataset.value;
    const group = container.getAttribute("data-group");
    if (group) setAllInGroup(group, value);
    else activate(container, value);
    writeQuery(container, value);
  });

  document.addEventListener("keydown", (e) => {
    const tab = e.target.closest('.tabs-container [role="tab"]');
    if (!tab) return;
    const container = tab.closest(".tabs-container");
    const { tabs } = getTabs(container);
    const idx = tabs.indexOf(tab);
    if (e.key === "ArrowRight" || e.key === "ArrowLeft") {
      e.preventDefault();
      const tgt =
        e.key === "ArrowRight"
          ? tabs[(idx + 1) % tabs.length]
          : tabs[(idx - 1 + tabs.length) % tabs.length];
      tgt?.focus();
    } else if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      tab.click();
    }
  });
}

export function onRouteDidUpdate() {
  if (!ExecutionEnvironment.canUseDOM) return;
  document
    .querySelectorAll(".tabs-container[data-tabs]")
    .forEach(initContainer);
}

if (ExecutionEnvironment.canUseDOM) {
  bindDelegatedHandlers();
  // also init on first load
  document.addEventListener("DOMContentLoaded", () => {
    document
      .querySelectorAll(".tabs-container[data-tabs]")
      .forEach(initContainer);
  });
}
