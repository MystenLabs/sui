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
      <div 
          className="overflow-hidden min-h-screen flex flex-col bg-cover bg-center bg-no-repeat"
          style={{
            backgroundColor: '#000000',
          }}
        >
          <div className="w-full mt-8 mb-4 mx-auto">
            <div className={styles.heroText}>
              <h1 className="h1 center-text text-white">Sui Documentation</h1>
              <h2 className="h2 center-text h3" style={{ color: '#89919F' }}>
                Discover the power of Sui through examples, guides, and concepts
              </h2>
            </div>
          </div>
          <div className="flex flex-row flex-wrap justify-center gap-2 max-w-[1066px] mx-auto pb-16 py-4">
            <HomeCard title="Getting Started">
              <Link
                className={`${styles.cardLink} plausible-event-name=homepage+start+button`}
                to="/getting-started/onboarding"
              >
                Hello, World!
              </Link>
              <Link className={styles.cardLink} to="/getting-started/tooling">
                Developer Tools
              </Link>
              <Link className={styles.cardLink} to="/getting-started/dev-cheat-sheet">
                Developer Cheat Sheet
              </Link>
            </HomeCard>
            <HomeCard title="Develop">
              <Link className={styles.cardLink} to="/develop/write-move">
                Writing Move Packages
              </Link>
              <Link className={styles.cardLink} to="/develop/objects">
                Using Objects
              </Link>
              <Link className={styles.cardLink} to="/develop/accessing-data">
                Accessing Data
              </Link>
            </HomeCard>
            <HomeCard title="Onchain Finance">
              <Link className={styles.cardLink} to="/onchain-finance/fungible-tokens">
                Fungible Tokens
              </Link>
              <Link className={styles.cardLink} to="/onchain-finance/tokenized-assets">
                Tokenized Assets
              </Link>
              <Link className={styles.cardLink} to="/onchain-finance/deepbookv3/deepbook">
                DeepBookV3
              </Link>
            </HomeCard>
            <HomeCard title="Sui Stack">
              <Link className={styles.cardLink} to="/sui-stack/nautilus">
                Nautilus
              </Link>
              <Link className={styles.cardLink} to="/sui-stack/zklogin-integration">
                zkLogin
              </Link>
              <Link className={styles.cardLink} to="/sui-stack/sagat">
                Sagat
              </Link>
            </HomeCard>
            <HomeCard title="References">
              <Link className={styles.cardLink} to="/references/cli">
                Sui CLI
              </Link>
              <Link className={styles.cardLink} to="/references/sui-api">
                Sui API
              </Link>
              <Link className={styles.cardLink} to="/references/framework">
                Move Framework
              </Link>
            </HomeCard>
            <HomeCard title="Node Operators">
              <Link className={styles.cardLink} to="/operators/full-node/sui-full-node">
                Run a Sui Full Node
              </Link>
              <Link className={styles.cardLink} to="/operators/validator">
                Validators
              </Link>
              <Link className={styles.cardLink} to="/operators/bridge-node-configuration">
                Bridge Node Configuration
              </Link>
            </HomeCard>
            <HomeCardCTA>
              <Link
                className={styles.cardCTALink}
                to="/getting-started/onboarding/hello-world"
              >
                <span>Build your first app on Sui</span>
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