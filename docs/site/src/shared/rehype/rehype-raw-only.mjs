
//
// This rehype plugin runs rehype-raw on MD/MDX files so raw HTML inside them gets parsed
// and merged into the HAST (the HTML AST).
// It also tells rehype-raw to ignore MDX nodes so it doesn’t try to compile them.

import rehypeRaw from 'rehype-raw';

const mdxPass = [
  'mdxjsEsm',
  'mdxJsxFlowElement',
  'mdxJsxTextElement',
  'mdxFlowExpression',
  'mdxTextExpression',
];

const rawForMd = rehypeRaw({passThrough: mdxPass});

export default function rehypeRawFiles() {
  return (tree, file) => {
    const p = (file?.path || '').toLowerCase();
    if (p.endsWith('.md', '.mdx')) {
      return rawForMd(tree, file);
    }
  };
}
