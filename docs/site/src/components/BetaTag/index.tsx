// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// This component is used to display beta tag.

import React from "react";
import Admonition from "@theme/Admonition";

export default function BetaTag(props) {
  if (!props.beta) {
    return;
  }

  const beta = props.beta.toLowerCase();

  return (
    <Admonition
      title="Beta Feature"
      icon="⚙️"
      className="!my-12 bg-sui-blue-light border-sui-blue-dark dark:bg-sui-blue-dark dark:border-sui-blue-light"
    >
      <p className="pt-2">
        The content in this topic describes a beta feature. Beta features are in
        active development, so feature details are subject to change before the
        feature is merged into Mainnet.
      </p>
      {(beta.includes("testnet") || beta.includes("devnet")) && (
        <p>
          This feature is currently available in
          <ul className="mt-4">
            {beta.includes("devnet") && <li className="font-bold">Devnet</li>}
            {beta.includes("testnet") && <li className="font-bold">Testnet</li>}
          </ul>
        </p>
      )}
    </Admonition>
  );
}
