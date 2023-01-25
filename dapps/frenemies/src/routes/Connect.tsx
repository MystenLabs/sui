// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ConnectButton, useWalletKit } from "@mysten/wallet-kit";
import { useEffect } from "react";
import { useNavigate } from "react-router-dom";
import { Card } from "../components/Card";

export function Connect() {
  const navigate = useNavigate();
  const { currentAccount } = useWalletKit();

  useEffect(() => {
    if (currentAccount) {
      navigate("/setup", { replace: true });
    }
  }, [currentAccount]);

  return (
    <div className="max-w-4xl w-full mx-auto text-center">
      <Card spacing="xl">
        <h1 className="text-steel-darker text-2xl leading-tight font-semibold mb-5">
          Welcome to Sui Frenemies game
        </h1>
        <img src="/capy_cowboy.svg" className="mb-5 h-64 w-64 mx-auto" />
        <div className="text-steel-dark uppercase leading-tight mb-1">
          Your Objective
        </div>
        <p className="text-steel-dark text-sm max-w-xs mb-12 mx-auto">
          The goal of the game is to stake Sui tokens to move your assigned
          Validator to one of three designated positions: Friend (top third),
          Neutral (middle third), or Foe (bottom third).
        </p>
        <ConnectButton
          connectText="Connect Wallet to participate"
          className="!bg-frenemies !text-white !shadow-notification !leading-none !px-5 !py-3 "
        />
      </Card>
    </div>
  );
}
