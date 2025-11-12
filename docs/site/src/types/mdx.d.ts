// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

declare module "*.mdx" {
  const MDXComponent: (props: any) => JSX.Element;
  export default MDXComponent;
}
declare module "*.md" {
  const MDXComponent: (props: any) => JSX.Element;
  export default MDXComponent;
}
