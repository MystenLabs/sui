/*
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
*/

import React from "react";

type Props = {
  href: string;
  children: React.ReactNode;
  className?: string;
};

/**
 * Renders either an internal anchor or an external <a target="_blank">.
 * Bypasses the broken-link checker since it's a component, not raw markdown.
 */
export default function UnsafeLink({ href, children, className }: Props) {
  const isExternal = /^https?:\/\//.test(href);

  if (isExternal) {
    return (
      <a
        href={href}
        target="_blank"
        rel="noopener noreferrer"
        className={className}
      >
        {children}
      </a>
    );
  }

  return (
    <a href={href} target="_self" className={className}>
      {children}
    </a>
  );
}
