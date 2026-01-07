
import React from "react";
import DocItem from "@theme-original/DocItem";
import GraphqlBetaLink from "@site/src/components/GraphqlBetaLink";
import { useLocation } from "@docusaurus/router";

export default function DocItemWrapper(props) {
  const doc = props?.content ?? {};
  const frontMatter = doc.frontMatter ?? {};
  const metadata = doc.metadata ?? {};

  const { pathname } = useLocation();
  const isGraphQlBeta = pathname?.includes("/sui-graphql/alpha/reference");
  const title = frontMatter?.title || metadata?.title || "GraphQL";

  return (
    <>
      {isGraphQlBeta && <GraphqlBetaLink title={title} />}
      <DocItem {...props} />
    </>
  );
}
