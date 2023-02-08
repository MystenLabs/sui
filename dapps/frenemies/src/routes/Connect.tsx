// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ConnectButton, useWalletKit } from "@mysten/wallet-kit";
import { ReactNode, useEffect } from "react";
import { useNavigate } from "react-router-dom";
import { Card } from "../components/Card";

function InfoItem({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div className="text-left">
      <div className="text-steel-darker text-heading6 font-semibold mb-1">
        {title}
      </div>
      <div className="text-p1 text-steel-dark">{children}</div>
    </div>
  );
}

function InfoLink({ href, children }: { href: string; children: ReactNode }) {
  return (
    <a
      href={href}
      target="_blank"
      rel="noopener noreferrer"
      className="text-frenemies font-semibold"
    >
      {children}
    </a>
  );
}

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
        <p className="text-steel-darker text-sm max-w-md mx-auto">
          The goal of the game is to stake Sui tokens to move your assigned
          Validator to one of three designated positions: Friend (top third),
          Neutral (middle third), or Enemy (bottom third).
        </p>
        <div className="h-px bg-steel/30 w-full my-8" />
        <div className="text-heading6 text-steel-darker">
          Here are a few things you will need to play this game.
        </div>
        <div className="mt-8 mb-12 grid grid-cols-1 sm:grid-cols-3 gap-x-10 gap-y-4">
          <InfoItem title="A Sui Wallet">
            You can download Sui Wallet from{" "}
            <InfoLink href="https://chrome.google.com/webstore/detail/sui-wallet/opcgpfmipidbgpenhmajoajpbobppdil">
              Chrome store here
            </InfoLink>
            .
          </InfoItem>
          <InfoItem title="Connect to Sui Testnet">
            Frenemies game only works on Testnet network.{" "}
            <InfoLink href="https://docs.sui.io/devnet/explore/wallet-browser#change-the-active-network">
              Learn more
            </InfoLink>
            .
          </InfoItem>
          <InfoItem title="SUI in your wallet">
            You can{" "}
            <InfoLink href="https://discord.com/channels/916379725201563759/1037811694564560966">
              request SUI on Discord
            </InfoLink>{" "}
            if you don't have some already.
          </InfoItem>
        </div>
        <ConnectButton
          connectText="Connect Wallet to participate"
          className="!bg-frenemies !text-white !shadow-notification !leading-none !px-5 !py-3 "
        />
      </Card>
    </div>
  );
}
