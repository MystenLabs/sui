// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React from "react";

import Layout from "@theme/Layout";
import Head from "@docusaurus/Head";
import Link from "@docusaurus/Link";
import styles from "./index.module.css";

// Target for the "Developer Updates" hero link. That page does not exist yet,
// so this points at the external Sui blog as an interim (external links are not
// route-checked, so the strict build passes). Swap this single constant to
// "/developer-updates" when the dedicated page ships.
export const DEVELOPER_UPDATES_URL = "https://blog.sui.io";

export default function Home() {
  const developerResources = [
    {
      title: "Getting Started",
      description:
        "Install Sui, set up your environment, and build your first app.",
      to: "/getting-started",
    },
    {
      title: "Develop",
      description:
        "Write Move packages, work with objects, and access onchain data.",
      to: "/develop",
    },
    {
      title: "Onchain Finance",
      description: "Tokens, stablecoins, payments, and NFTs.",
      to: "/onchain-finance",
    },
    {
      title: "Sui Stack",
      description: "Onchain primitives like Nautilus, zkLogin, and Seal.",
      to: "/sui-stack",
    },
    {
      title: "References",
      description: "CLI, SDKs, framework, and API reference.",
      to: "/references",
    },
    {
      title: "Sui Agent Skills",
      description: "Sui skills for AI coding agents.",
      to: "/skills",
    },
  ];

  const useCases = [
    {
      title: "DeepBook",
      description: "Sui's onchain central limit order book for spot and margin.",
      to: "/onchain-finance/deepbook",
    },
    {
      title: "Walrus",
      description: "Decentralized storage for app data and media.",
      to: "/sui-stack/walrus",
    },
    {
      title: "SuiPlay0X1",
      description: "Build games for the SuiPlay0X1 handheld.",
      to: "/sui-stack/suiplay0x1",
    },
  ];

  const nodeOperators = [
    {
      title: "Run a Sui Full Node",
      description: "Set up and run a Sui full node.",
      to: "/operators/full-node/sui-full-node",
    },
    {
      title: "Validators",
      description: "Operate and maintain a Sui validator.",
      to: "/operators/validator",
    },
    {
      title: "Bridge Node Configuration",
      description: "Configure a node for Sui Bridge.",
      to: "/operators/bridge-node-configuration",
    },
  ];

  const ResourceCard = ({ title, description, to }) => (
    <Link to={to} className={styles.resourceCard}>
      <h3 className={styles.resourceCardTitle}>{title}</h3>
      <p className={styles.resourceCardDesc}>{description}</p>
      <span className={styles.resourceCardArrow} aria-hidden="true">
        →
      </span>
    </Link>
  );

  const Section = ({ heading, items }) => (
    <section className={styles.homeSection}>
      <h2 className={styles.homeSectionHeading}>{heading}</h2>
      <div className="flex flex-row flex-wrap justify-center gap-2">
        {items.map((item) => (
          <ResourceCard key={item.title} {...item} />
        ))}
      </div>
    </section>
  );

  return (
    <>
      <Head>
        <meta
          name="google-site-verification"
          content="nOyG5Cxvr3m94VHwQFHHaK_5BR6EyAYJ_4oPxYBptPs"
        />
      </Head>
      <Layout>
        <div
          className="overflow-hidden min-h-screen flex flex-col bg-cover bg-center bg-no-repeat"
          style={{ backgroundColor: "#000000" }}
        >
          <div className="w-full mt-8 mb-4 mx-auto">
            <div className={styles.heroText}>
              <h1 className="h1 center-text text-white">Sui Documentation</h1>
              <p
                className="center-text"
                style={{
                  color: "#89919F",
                  maxWidth: "720px",
                  margin: "0 auto",
                  fontSize: "1.1rem",
                  lineHeight: "1.6",
                }}
              >
                Sui is a next-generation smart contract platform with high
                throughput, low latency, and an asset-oriented programming model
                powered by the Move programming language. Explore guides,
                references, and tutorials to start building on Sui.
              </p>
              <Link to={DEVELOPER_UPDATES_URL} className={styles.devUpdates}>
                Developer Updates
              </Link>
            </div>
          </div>

          <Section heading="Developer Resources" items={developerResources} />
          <Section heading="Use Cases" items={useCases} />
          <Section heading="Node Operators" items={nodeOperators} />
        </div>
      </Layout>
    </>
  );
}
