// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// This component is used as part of the effort plugin.
// It appears at the top of any guide with an `effort`
// rating (small, medium, large) in its frontmatter.

import React from "react";
import Admonition from "@theme/Admonition";

export default function EffortBox(props) {
  if (!props.effort) {
    return;
  }
  const effort =
    props.effort[0] === "s"
      ? "basic"
      : props.effort[0] === "m"
        ? "involved"
        : "advanced";
  const time =
    effort === "basic"
      ? "30-45 minutes"
      : effort === "involved"
        ? "1-1.5 hours"
        : "2 hours or more";
  return (
    <Admonition
      title="Effort"
      icon="🧠"
      className="!my-12 bg-sui-ghost-white border-sui-ghost-dark dark:bg-sui-ghost-dark dark:border-sui-ghost-white"
    >
      <p className="pt-2">
        This guide is rated as <span className="font-bold">{effort}</span>.
      </p>
      <p>
        You can expect {effort} guides to take{" "}
        <span className="font-bold">{time}</span> of dedicated time. The length
        of time necessary to fully understand some of the concepts raised in
        this guide might increase this estimate.
      </p>
    </Admonition>
  );
}
