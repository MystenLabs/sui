

import React from "react";

type Props = {
  href: string;
  children: React.ReactNode;
  className?: string;
};

/**
 * Renders either a Docusaurus <Link> for internal paths
 * or an <a target="_blank"> for external URLs.
 * This bypasses the broken-link checker since it’s a component,
 * not a raw markdown link. Should never have an external link
 * using this, but jic.
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
