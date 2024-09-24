// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React from "react";
import DefaultAdmonitionTypes from "@theme-original/Admonition/Types";

function CheckpointAdmonition(props) {
  return (
    <div className="bg-sui-ghost-white dark:bg-sui-blue-dark my-8 pr-4 border border-solid border-sui-blue-dark dark:border-sui-blue rounded-lg flex">
      <div className="flex-none w-[21px] mr-4 dark:bg-checkerboard bg-checkerboard-dark"></div>
      <div className="my-4">
        <div className="font-bold">
          <span className="text-sm">CHECKPOINT</span>
        </div>
        <h5 style={{ color: "blue", fontSize: 30 }}>{props.title}</h5>
        <div>{props.children}</div>
      </div>
    </div>
  );
}

const AdmonitionTypes = {
  ...DefaultAdmonitionTypes,

  // Add all your custom admonition types here...
  // You can also override the default ones if you want
  checkpoint: CheckpointAdmonition,
};

export default AdmonitionTypes;
