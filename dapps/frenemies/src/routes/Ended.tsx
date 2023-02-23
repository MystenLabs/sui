// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Card } from "../components/Card";

export function Ended() {
  return (
    <div className="max-w-xl mx-auto w-full text-center">
      <Card spacing="2xl">
        <div>
          <img src="/capy_cowboy.svg" alt="Capy" className="block mx-auto" />
          <h1 className="mt-4 text-steel-darker text-heading2 font-semibold leading-tight">
            Thank You For Playing Frenemies!
          </h1>
          <div className="mt-4 text-steel-darker text-heading6 leading-normal">
            Sui Testnet Wave 2 ended Wednesday February 15 at 4pm PST. Thank you
            for playing!
          </div>
        </div>
      </Card>
    </div>
  );
}
