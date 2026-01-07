
import React from "react";

/**
 * We glob-import every MDX file under @site/snippets/**.
 * Docusaurus (Webpack 5) supports require.context at build time.
 */
// eslint-disable-next-line @typescript-eslint/no-var-requires
const req = require.context("../../../../content/snippets", true, /\.mdx$/);

type SnippetModule = { default: React.ComponentType<any> };

const SNIPPETS: Record<string, React.ComponentType<any>> = {};
req.keys().forEach((k: string) => {
  const mod = req<SnippetModule>(k);
  const keyWithExt = k.replace(/^\.\//, ""); // e.g. "sub/foo.mdx"
  const keyNoExt = keyWithExt.replace(/\.mdx$/, ""); // e.g. "sub/foo"
  SNIPPETS[keyWithExt] = mod.default;
  SNIPPETS[keyNoExt] = mod.default;
});

type Props = {
  /** Path under snippets/, e.g. "subfolder-of-snippet/file" or "subfolder-of-snippet/file.mdx" */
  source: string;
} & Record<string, any>;

/**
 * Renders the MDX snippet inline, inheriting parent MDX providers
 * (so code blocks, custom components, etc. all work).
 */
export default function Snippet({ source, ...rest }: Props) {
  const Comp =
    SNIPPETS[source] ||
    SNIPPETS[`${source}.mdx`] ||
    SNIPPETS[source.replace(/^\.\//, "")];

  if (!Comp) {
    return (
      <div className="alert alert--warning" role="alert">
        Missing snippet: <code>{source}</code>
      </div>
    );
  }

  // Render the snippet's MDX component inline
  return <Comp {...rest} />;
}
