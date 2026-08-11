// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React from "react";
import type { Props } from "@theme/CodeBlock";
import OriginalCodeBlock from "@theme-original/CodeBlock";

export default function CodeBlock(props: Props) {
  const meta = (props as any).metastring ?? "";
  const match = meta.match(/agentPrompt="([^"]+)"/);
  const prompt = match?.[1];

  return (
    <div data-agent-prompt={prompt}>
      <OriginalCodeBlock {...props} />
    </div>
  );
}
