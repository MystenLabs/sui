// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React from "react";

import Layout from "@theme/Layout";
import Head from "@docusaurus/Head";
import Link from "@docusaurus/Link";
import styles from "./index.module.css";

export default function Home() {
  const HomeCard = (props) => {
    const { title, children } = props;
    return (
      <div className={`p-px col-span-3 w-[350px]`}>
        <div className={styles.card}>
          {title && <h4 className="h4 text-white">{title}</h4>}
          <div className={styles.cardLinksContainer}>{children}</div>
        </div>
      </div>
    );
  };
  const HomeCardCTA = (props) => {
    const { children } = props;
    return (
      <div className={`p-px col-span-3 w-[350px]`}>
        <div className={styles.cardCTA}>
          <div className={styles.cardLinksContainer}>{children}</div>
        </div>
      </div>
    );
  };

  return (
    <>
      <Head>
        <meta
          name="google-site-verification"
          content="nOyG5Cxvr3m94VHwQFHHaK_5BR6EyAYJ_4oPxYBptPs"
        />
      </Head>
      <Layout>
        <div className="bg-sui-black overflow-hidden min-h-screen flex flex-col">
          <div className="w-full mt-8 mb-4 mx-auto bg-sui-black">
            <div className={styles.heroText}>
              <h1 className="h1 center-text text-white">Sui Documentation</h1>
              <h2 className="h2 text-gray center-text h3">
                Discover the power of Sui through examples, guides, and concepts
              </h2>
            </div>
          </div>
          <div className="flex flex-row flex-wrap justify-center gap-2 max-w-[1066px] mx-auto pb-16 py-4">
            <HomeCard title="Developers">
              <Link
                className={`${styles.cardLink} plausible-event-name=homepage+start+button`}
                to="./guides/developer/getting-started/sui-install"
              >
                Getting Started
              </Link>
              <Link className={styles.cardLink} to="./guides/developer/sui-101">
                Sui Developer Basics
              </Link>
              <Link
                className={styles.cardLink}
                to="./concepts/sui-move-concepts"
              >
                Move
              </Link>
            </HomeCard>
            <HomeCard title="Validators and Node operators">
              <Link
                className={styles.cardLink}
                to="./guides/operator/validator/validator-config"
              >
                Validator Configuration
              </Link>
              <Link
                className={styles.cardLink}
                to="./guides/operator/sui-full-node"
              >
                Run a Sui Full Node
                <span className="block bg-auto bg-[url(../static/img/index/right-arrow.svg)]"></span>
              </Link>
              <Link
                className={styles.cardLink}
                to="./guides/operator/bridge-node-configuration"
              >
                Sui Bridge Node Configuration
              </Link>
            </HomeCard>
            <HomeCard title="About Sui">
              <Link className={styles.cardLink} to="./concepts/tokenomics">
                Tokenomics
              </Link>
              <Link className={styles.cardLink} to="./concepts/cryptography">
                Cryptography
              </Link>
              <Link className={styles.cardLink} to="standards">
                Standards
              </Link>
            </HomeCard>
            <HomeCard title="References" aux>
              <Link
                className={styles.cardLink}
                to="https://sdk.mystenlabs.com/dapp-kit?ref=blog.sui.io"
              >
                Sui dApp Kit
              </Link>
              <Link className={styles.cardLink} to="/references/sui-api">
                Sui API
              </Link>
              <Link
                className={styles.cardLink}
                to="https://github.com/MystenLabs/sui/tree/main/crates/sui-framework/docs"
              >
                Sui Framework
              </Link>
              <Link
                className={styles.cardLink}
                to="https://github.com/MystenLabs/sui/tree/main/crates/sui-sdk"
              >
                Rust SDK
              </Link>
            </HomeCard>
            <HomeCard title="Resources" aux>
              <Link
                className={styles.cardLink}
                to="https://sui.directory/?_project_type=api%2Cdeveloper-tools%2Cinfrastructure%2Csdk"
              >
                Sui Ecosystem
              </Link>
              <Link className={styles.cardLink} to="/references/awesome-sui">
                Awesome Sui
              </Link>
              <Link className={styles.cardLink} to="https://blog.sui.io/">
                Sui blog
              </Link>
              <Link
                className={styles.cardLink}
                to="guides/developer/dev-cheat-sheet"
              >
                Sui Developer Cheat Sheet
              </Link>
            </HomeCard>
            <HomeCardCTA>
              <Link
                className={styles.cardCTALink}
                to="/guides/developer/getting-started/hello-world"
              >
                <span>Build your dApp on Sui</span>
                <svg
                  width="11"
                  height="11"
                  viewBox="0 0 11 11"
                  fill="none"
                  xmlns="http://www.w3.org/2000/svg"
                >
                  <path
                    d="M6.01312 0.5L5.05102 1.45391L8.39164 4.80332L0 4.80332L0 6.19668L8.39164 6.19668L5.05102 9.54073L6.01312 10.5L11 5.5L6.01312 0.5Z"
                    fill="#298DFF"
                  />
                </svg>
              </Link>
            </HomeCardCTA>
          </div>
        </div>
      </Layout>
    </>
  );
}