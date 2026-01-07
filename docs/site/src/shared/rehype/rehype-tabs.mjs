/*
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
*/
//
// Rehype plugin: transforms <tabs>/<tabitem> (from MD or MDX) into the
// same DOM that Docusaurus Tabs renders—plus small, scoped classes
// to force consistent vertical alignment across all tab items.

import { visit } from 'unist-util-visit';

function slug(s) {
  return (
    String(s || '')
      .trim()
      .toLowerCase()
      .replace(/[^a-z0-9\-_.]+/g, '-')
      .replace(/^-+|-+$/g, '') || 'tab'
  );
}

function cloneChildren(nodes) {
  return nodes ? JSON.parse(JSON.stringify(nodes)) : [];
}

function classes(...vals) {
  const out = [];
  for (const v of vals) {
    if (!v) continue;
    if (Array.isArray(v)) out.push(...v.flat().filter(Boolean));
    else if (typeof v === 'string') out.push(...v.split(/\s+/).filter(Boolean));
    else if (v.className) return classes(out, v.className);
  }
  return Array.from(new Set(out));
}

function getFromHast(props = {}, ...names) {
  for (const n of names) if (props[n] != null) return String(props[n]);
}
function hasFlagHast(props = {}, ...names) {
  for (const n of names) if (n in props) return true;
  return false;
}
function getFromMdx(node, ...names) {
  const attrs = node.attributes || [];
  for (const n of names) {
    const a = attrs.find((x) => x?.type === 'mdxJsxAttribute' && x.name === n);
    if (!a) continue;
    if (a.value == null) return ''; // boolean attr present
    if (typeof a.value === 'string') return a.value;
  }
}
function hasFlagMdx(node, ...names) {
  const attrs = node.attributes || [];
  return names.some((n) =>
    attrs.some((x) => x?.type === 'mdxJsxAttribute' && x.name === n),
  );
}
function getClassMdx(node) {
  const v = getFromMdx(node, 'className', 'class');
  return v ? classes(v) : [];
}

let seq = 0;

export default function rehypeTabsMd() {
  return (tree) => {
    visit(tree, (node, index, parent) => {
      if (!parent) return;

      const isHastTabs = node.type === 'element' && node.tagName === 'tabs';
      const isMdxTabs =
        node.type === 'mdxJsxFlowElement' &&
        typeof node.name === 'string' &&
        node.name.toLowerCase() === 'tabs';

      if (!isHastTabs && !isMdxTabs) return;

      const props = isHastTabs ? node.properties || {} : {};
      const groupId = isHastTabs
        ? getFromHast(props, 'groupid', 'groupId') || `tabs-${++seq}`
        : getFromMdx(node, 'groupid', 'groupId') || `tabs-${++seq}`;

      const defaultValue = isHastTabs
        ? getFromHast(props, 'defaultvalue', 'defaultValue')
        : getFromMdx(node, 'defaultvalue', 'defaultValue');

      const queryString = isHastTabs
        ? hasFlagHast(props, 'querystring', 'queryString', 'data-querystring')
        : hasFlagMdx(node, 'querystring', 'queryString', 'data-querystring');

      const block = isHastTabs
        ? hasFlagHast(props, 'block')
        : hasFlagMdx(node, 'block');

      const containerExtraClass = isHastTabs
        ? classes(props.className)
        : getClassMdx(node);

      // Collect <tabitem> children
      const ch = node.children || [];
      const tabNodes = ch.filter((c) => {
        if (!c) return false;
        if (c.type === 'element' && c.tagName === 'tabitem') return true;
        if (
          c.type === 'mdxJsxFlowElement' &&
          typeof c.name === 'string' &&
          c.name.toLowerCase() === 'tabitem'
        )
          return true;
        return false;
      });
      if (!tabNodes.length) return;

      const items = tabNodes.map((child, i) => {
        if (child.type === 'element') {
          const p = child.properties || {};
          return {
            value: getFromHast(p, 'value') || `item-${i + 1}`,
            label:
              getFromHast(p, 'label') ||
              getFromHast(p, 'value') ||
              `item-${i + 1}`,
            itemClass: classes(p.className),
            content: cloneChildren(child.children),
          };
        } else {
          const value = getFromMdx(child, 'value') || `item-${i + 1}`;
          const label = getFromMdx(child, 'label') || value;
          return {
            value,
            label,
            itemClass: getClassMdx(child),
            content: cloneChildren(child.children),
          };
        }
      });

      const selectedValue =
        defaultValue && items.some((it) => it.value === defaultValue)
          ? defaultValue
          : items[0].value;

      const base = slug(groupId);
      const nameAttr = `md-tabs-${base}`;

      // Build <li role="tab"> items
      const liEls = items.map((it) => {
        const vSlug = slug(it.value);
        const tabId = `${nameAttr}-${vSlug}-tab`;
        const panelId = `${nameAttr}-${vSlug}-panel`;
        const selected = it.value === selectedValue;
        return {
          type: 'element',
          tagName: 'li',
          properties: {
            role: 'tab',
            id: tabId,
            tabIndex: selected ? 0 : -1,
            'aria-selected': selected ? 'true' : 'false',
            'aria-controls': panelId,
            'data-value': it.value,
            className: classes(
              'tabs__item',
              'tabs__item--mdx', // <— scoped helper: force block flex + center
              selected && 'tabs__item--active',
              it.itemClass,
            ),
          },
          children: [
            {
              type: 'element',
              tagName: 'span',
              properties: { className: ['tabs__label'] },
              children: [{ type: 'text', value: it.label }],
            },
          ],
        };
      });

      // Build tab panels
      const panelEls = items.map((it) => {
        const vSlug = slug(it.value);
        const tabId = `${nameAttr}-${vSlug}-tab`;
        const panelId = `${nameAttr}-${vSlug}-panel`;
        const selected = it.value === selectedValue;
        return {
          type: 'element',
          tagName: 'div',
          properties: {
            id: panelId,
            role: 'tabpanel',
            'aria-labelledby': tabId,
            'data-value': it.value,
            hidden: selected ? undefined : true,
          },
          children: it.content,
        };
      });

      // Replacement tree
      const replacement = {
        type: 'element',
        tagName: 'div',
        properties: {
          className: classes(
            'tabs-container',
            'tabs__container--mdx', // <— scoped helper class
            containerExtraClass,
          ),
          'data-md-tabs': '',
          'data-group': groupId,
          ...(queryString ? { 'data-querystring': '' } : {}),
        },
        children: [
          {
            type: 'element',
            tagName: 'ul',
            properties: {
              role: 'tablist',
              'aria-orientation': 'horizontal',
              className: classes(
                'tabs',
                'tabs__list--mdx', // <— scoped helper: consistent layout
                block && 'tabs--block',
              ),
            },
            children: liEls,
          },
          {
            type: 'element',
            tagName: 'div',
            properties: { className: ['margin-top--md'] },
            children: panelEls,
          },
        ],
      };

      parent.children.splice(index, 1, replacement);
      return [visit.SKIP, index + 1];
    });
  };
}
