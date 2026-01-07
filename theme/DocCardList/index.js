
import React from 'react';
import DocCardList from '@theme/DocCardList';
import { useCurrentSidebarCategory } from '@docusaurus/theme-common';

// Renders the current sidebar category *if* we're inside DocsSidebarProvider,
// otherwise falls back to a plain DocCardList using passed props (items/hideDescriptions/etc).
export default function DocCardListForCurrentSidebarCategory(props) {
  try {
    // eslint-disable-next-line react-hooks/rules-of-hooks
    const category = useCurrentSidebarCategory(); // throws outside provider
    return <DocCardList items={category.items} />;
  } catch {
    // Outside docs provider (e.g., homepage or custom MDX): use explicit props
    // Example MDX usage: <DocCardList items={[{type:'link', label:'Intro', href:'/docs/intro'}]} />
    return <DocCardList {...props} />;
  }
}


