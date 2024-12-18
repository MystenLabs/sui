// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React from "react";

import Layout from "@theme/Layout";
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
    <Layout>
      <div className="bg-sui-black overflow-hidden">
        <div className="w-full mt-24 mb-12 mx-auto bg-sui-black">
          <div className={styles.heroText}>
            <h1 className="h1 center-text text-white">Sui Documentation</h1>
            <h2 className="h2 text-gray center-text h3">
              Discover the power of Sui through examples, guides, and concepts
            </h2>
          </div>
        </div>
        <div className="flex flex-row flex-wrap justify-center gap-2 max-w-[1066px] mx-auto">
          <HomeCard title="Developers">
            <Link
              className={styles.cardLink}
              to="./guides/developer/getting-started"
            >
              Getting started
            </Link>
            <Link className={styles.cardLink} to="./guides/developer/sui-101">
              Sui Developer Basics
            </Link>
            <Link className={styles.cardLink} to="./concepts/sui-move-concepts">
              Move
            </Link>
          </HomeCard>
          <HomeCard title="Validators and Node operators">
            <Link
              className={styles.cardLink}
              to="./guides/operator/validator-config"
            >
              Validator configuration
            </Link>
            <Link
              className={styles.cardLink}
              to="./guides/operator/sui-full-node"
            >
              Run a Sui Full node
              <span className="block bg-auto bg-[url(../static/img/index/right-arrow.svg)]"></span>
            </Link>
            <Link
              className={styles.cardLink}
              to="./guides/operator/bridge-node-configuration"
            >
              Sui Bridge Node configuration
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
              Sui framework (GitHub)
            </Link>
            <Link
              className={styles.cardLink}
              to="https://github.com/MystenLabs/sui/tree/main/crates/sui-sdk"
            >
              Rust SDK (GitHub)
            </Link>
          </HomeCard>
          <HomeCard title="Resources" aux>
            <Link
              className={styles.cardLink}
              to="https://sui.directory/?_project_type=api%2Cdeveloper-tools%2Cinfrastructure%2Csdk"
            >
              Sui ecosystem directory
            </Link>
            <Link className={styles.cardLink} to="https://blog.sui.io/">
              Sui blog
            </Link>
            <Link
              className={styles.cardLink}
              to="guides/developer/dev-cheat-sheet"
            >
              Sui dev cheat sheet
            </Link>
          </HomeCard>
          <HomeCardCTA>
            <Link
              className={styles.cardCTALink}
              to="/guides/developer/first-app"
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
                  fill="#4DA2FF"
                />
              </svg>
            </Link>
          </HomeCardCTA>
        </div>

        <div className={styles.sectionHeader}>
          <h2 className="h1 text-white font-twkeverett">Why Sui?</h2>
          <h3 className="h3 text-gray text-center">
            Sui is the first internet-scale programmable blockchain platform
          </h3>
        </div>
        <div className={styles.why}>
          <div className="">
            <img
              height={"100%"}
              src="/img/index/blocks.png"
              alt="Decorative visual"
            />
          </div>
          <div className={styles.cardsB}>
            <div className={styles.cardB}>
              <svg
                width="32"
                height="32"
                viewBox="0 0 32 32"
                fill="none"
                xmlns="http://www.w3.org/2000/svg"
              >
                <path
                  d="M17.3337 3.99902V13.3324H25.3337L14.667 27.999V18.6657H6.66699L17.3337 3.99902Z"
                  stroke="#C0DEFF"
                  strokeWidth="2"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                />
              </svg>
              <span>Unmatched scalability, instant settlement</span>
            </div>
            <div className={styles.cardB}>
              <svg
                width="32"
                height="32"
                viewBox="0 0 32 32"
                fill="none"
                xmlns="http://www.w3.org/2000/svg"
              >
                <path
                  d="M12.5664 12H15.5996"
                  stroke="#C0DEFF"
                  strokeWidth="2"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                />
                <path
                  d="M12.5664 17.333H22.5171"
                  stroke="#C0DEFF"
                  strokeWidth="2"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                />
                <path
                  d="M12.5664 22.667H22.5171"
                  stroke="#C0DEFF"
                  strokeWidth="2"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                />
                <rect
                  x="8.76855"
                  y="3.67871"
                  width="20.6312"
                  height="24.6722"
                  rx="2"
                  stroke="#C0DEFF"
                  strokeWidth="2"
                />
                <path
                  d="M8.60445 17.6113L3.21655 17.6113C3.09911 17.6113 3.00391 17.7065 3.00391 17.824V25.4746C3.00391 27.0627 4.29131 28.3501 5.87941 28.3501V28.3501C7.46751 28.3501 8.75492 27.0627 8.75492 25.4746V23.1274"
                  stroke="#C0DEFF"
                  strokeWidth="2"
                  strokeLinecap="round"
                />
                <path
                  d="M6.20703 28.3496H13.3685"
                  stroke="#C0DEFF"
                  strokeWidth="2"
                  strokeLinecap="round"
                />
              </svg>

              <span>
                A safe smart contract language accessible to mainstream
                developers
              </span>
            </div>
            <div className={styles.cardB}>
              <svg
                width="32"
                height="32"
                viewBox="0 0 32 32"
                fill="none"
                xmlns="http://www.w3.org/2000/svg"
              >
                <path
                  d="M16 20.0007H7.33333C6.44928 20.0007 5.60143 19.6495 4.97631 19.0243C4.35119 18.3992 4 17.5514 4 16.6673C4 15.7833 4.35119 14.9354 4.97631 14.3103C5.60143 13.6852 6.44928 13.334 7.33333 13.334H8"
                  stroke="#C0DEFF"
                  strokeWidth="2"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                />
                <path
                  d="M20.0002 16V24.6667C20.0002 25.5507 19.649 26.3986 19.0239 27.0237C18.3987 27.6488 17.5509 28 16.6668 28C15.7828 28 14.9349 27.6488 14.3098 27.0237C13.6847 26.3986 13.3335 25.5507 13.3335 24.6667V24"
                  stroke="#C0DEFF"
                  strokeWidth="2"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                />
                <path
                  d="M16 12H24.6667C25.5507 12 26.3986 12.3512 27.0237 12.9763C27.6488 13.6014 28 14.4493 28 15.3333C28 16.2174 27.6488 17.0652 27.0237 17.6904C26.3986 18.3155 25.5507 18.6667 24.6667 18.6667H24"
                  stroke="#C0DEFF"
                  strokeWidth="2"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                />
                <path
                  d="M12 16V7.33333C12 6.44928 12.3512 5.60143 12.9763 4.97631C13.6014 4.35119 14.4493 4 15.3333 4C16.2174 4 17.0652 4.35119 17.6904 4.97631C18.3155 5.60143 18.6667 6.44928 18.6667 7.33333V8"
                  stroke="#C0DEFF"
                  strokeWidth="2"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                />
              </svg>

              <span>Ability to define rich and composable on-chain assets</span>
            </div>
            <div className={styles.cardB}>
              <svg
                width="32"
                height="33"
                viewBox="0 0 32 33"
                fill="none"
                xmlns="http://www.w3.org/2000/svg"
              >
                <rect
                  x="4.21191"
                  y="5.48926"
                  width="9.42373"
                  height="9.42373"
                  rx="2"
                  stroke="#C0DEFF"
                  strokeWidth="2"
                />
                <rect
                  x="16.4422"
                  y="8.47931"
                  width="9.42373"
                  height="9.42373"
                  rx="2"
                  transform="rotate(-30 16.4422 8.47931)"
                  stroke="#C0DEFF"
                  strokeWidth="2"
                />
                <rect
                  x="4.21191"
                  y="19.4453"
                  width="9.42373"
                  height="9.42373"
                  rx="2"
                  stroke="#C0DEFF"
                  strokeWidth="2"
                />
                <rect
                  x="18.166"
                  y="19.4453"
                  width="9.42373"
                  height="9.42373"
                  rx="2"
                  stroke="#C0DEFF"
                  strokeWidth="2"
                />
              </svg>

              <span>Better user experience for web3 apps</span>
            </div>
          </div>
        </div>
        <div className={styles.TwoColParagraph}>
          <div className={styles.TwoColItem}>
            <span>Scalability</span>
            <p>
              Sui scales horizontally to meet the demands of applications.
              Network capacity grows in proportion to the increase in Sui
              validators' processing power by adding workers, resulting in low
              gas fees even during high network traffic. This scalability
              characteristic is in sharp contrast to other blockchains with
              rigid bottlenecks.
            </p>
          </div>
          <div className={styles.TwoColItem}>
            <span>Move</span>
            <p>
              Move design prevents issues such as reentrancy vulnerabilities,
              poison tokens, and spoofed token approvals that attackers have
              leveraged to steal millions on other platforms. The emphasis on
              safety and expressivity provides a more straightforward transition
              from web 2.0 to web3 for developers, without the need to
              understand the intricacies of the underlying infrastructure.
            </p>
          </div>
          <div className={styles.TwoColItem}>
            <span>On-chain assets</span>
            <p>
              Rich on-chain assets enable new applications and economies based
              on utility without relying solely on artificial scarcity.
              Developers can implement dynamic NFTs that you can upgrade,
              bundle, and group in an application-specific manner, such as
              changes in avatars and customizable items based on gameplay. This
              capability delivers stronger in-game economies as NFT behavior
              gets fully reflected on-chain, making NFTs more valuable and
              delivering more engaging feedback loops.
            </p>
          </div>
          <div className={styles.TwoColItem}>
            <span>Built for Web3</span>
            <p>
              Sui aims to be the most accessible smart contract platform,
              empowering developers to create great user experiences in web3. To
              usher in the next billion users, Sui empowers developers with
              various tools to take advantage of the power of the Sui
              blockchain. The Sui Development Kit (SDK) will enable developers
              to build without boundaries.
            </p>
          </div>
        </div>
      </div>
    </Layout>
  );
}
