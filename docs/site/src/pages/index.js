// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React, { useState } from "react";
import { useHistory } from "@docusaurus/router";

import Layout from "@theme/Layout";
import Link from "@docusaurus/Link";

export default function Home() {
  const history = useHistory();
  const SingleLink = (props) => {
    const { to } = props;

    return (
      <div
        onClick={() => history.push(to)}
        className="bg-sui-card-dark rounded-sui w-[350px] h-[350px] p-8 bg-[url(../static/img/index/card-bg.svg)] justify-center flex justify-center items-center"
      >
        <div className="p-4 rounded-full border border-solid border-sui-white/30 w-[249px] h-[51px] text-sui-white bg-sui-card-dark bg-[url(../static/img/index/circle-arrow.svg)] bg-no-repeat bg-[center_right_2px] flex items-center cursor-pointer hover:shadow-sui hover:shadow-sui-blue hover:bg-opacity-50 hover:bg-[url(../static/img/index/circle-arrow-blue.svg)]">
          Build your first dApp
        </div>
      </div>
    );
  };
  const ContentItem = (props) => {
    const [vis, setVis] = useState(false);
    const { children, title } = props;
    const handleClick = () => {
      setVis(!vis);
    };

    return (
      <div className="grid grid-cols-3 border-solid border-0 border-t border-sui-white/50 mb-8 lg:mx-0 mx-4">
        <p
          className={`lg:text-4xl text-2xl pb-2 mt-8 pr-12 cursor-pointer bg-no-repeat bg-right-top flex-none ${
            vis
              ? "bg-[url(../static/img/index/circle-arrow-up.svg)]"
              : "bg-[url(../static/img/index/circle-arrow-down.svg)]"
          }`}
          onClick={handleClick}
        >
          {title}
        </p>
        <p
          className={`${
            vis ? "opacity-100" : "opacity-0"
          } transition-opacity ease-in-out duration-300 col-span-2 mt-10 text-lg text-sui-blue-lighter mx-8`}
        >
          {vis && children}
        </p>
      </div>
    );
  };
  const HomeCard = (props) => {
    const { aux, title, children } = props;
    return (
      <div
        className={`p-px col-span-3 bg-gradient-to-b from-sui-white/40 from-20% hover:from-30% via-sui-white/0 to-sui-white/10 to-80% rounded-sui w-[350px] h-[350px]`}
      >
        <div
          className={`${
            aux ? "bg-sui-ghost-dark" : "bg-sui-card-dark"
          } rounded-sui w-full h-full p-8 max-w-[350px] max-h-[350px]`}
        >
          <p className="text-4xl text-white table-cell align-bottom pb-2 h-16 pb-8">
            {title}
          </p>
          {children}
        </div>
      </div>
    );
  };

  const cardlinks =
    "block py-3 border-0 border-t border-solid border-color-white text-sui-blue-lighter bg-[url(../static/img/index/right-arrow.svg)] bg-no-repeat bg-right hover:no-underline hover:text-sui-blue-lighter hover:bg-[url(../static/img/index/right-arrow-blue.svg)] pr-10";
  const darkcardclass =
    "p-px col-span-4 bg-gradient-to-b from-sui-white/40 from-20% hover:from-30% via-sui-white/0 to-sui-white/10 to-80% rounded-sui";
  return (
    <Layout>
      <div className="bg-sui-black overflow-hidden">
        <div className="w-full mt-24 mb-12 mx-auto bg-sui-black">
          <div className="text-center">
            <p className="lg:text-8xl md:text-6xl text-4xl text-white">
              Sui Documentation
            </p>
            <p className="text-lg sm:text-xl lg:text-3xl mb-0 px-8 md:w-1/2 w-auto mx-auto text-sui-blue-lighter inline-block">
              Discover the power of Sui through examples, guides, and concepts
            </p>
          </div>
        </div>
        <div className="flex flex-row flex-wrap justify-center gap-2 max-w-[1066px] mx-auto">
          <HomeCard title="About Sui">
            <Link
              className={`${cardlinks} text-2xl`}
              to="./concepts/tokenomics"
            >
              Tokenomics
            </Link>
            <Link
              className={`${cardlinks} text-2xl`}
              to="./concepts/cryptography"
            >
              Cryptography
            </Link>
            <Link className={`${cardlinks} text-2xl`} to="standards">
              Standards
            </Link>
          </HomeCard>
          <HomeCard title="Developers">
            <Link
              className={`${cardlinks} text-2xl`}
              to="./guides/developer/getting-started"
            >
              Getting started
            </Link>
            <Link
              className={`${cardlinks} text-2xl`}
              to="./guides/developer/sui-101"
            >
              Sui Developer Basics
            </Link>
            <Link
              className={`${cardlinks} text-2xl`}
              to="./concepts/sui-move-concepts"
            >
              Move
            </Link>
          </HomeCard>
          <HomeCard title="Validators and Node operators">
            <Link
              className={`${cardlinks} text-2xl`}
              to="./guides/operator/validator-config"
            >
              Validator configuration
            </Link>
            <Link
              className={`${cardlinks} text-2xl`}
              to="./guides/operator/sui-full-node"
            >
              Run a Sui Full node
              <span className="block bg-auto bg-[url(../static/img/index/right-arrow.svg)]"></span>
            </Link>
          </HomeCard>
          <HomeCard title="References" aux>
            <Link
              className={`${cardlinks} text-md`}
              to="https://sdk.mystenlabs.com/dapp-kit?ref=blog.sui.io"
            >
              Sui dApp Kit
            </Link>
            <Link className={`${cardlinks} text-md`} to="/references/sui-api">
              Sui API
            </Link>
            <Link
              className={`${cardlinks} text-md`}
              to="https://github.com/MystenLabs/sui/tree/main/crates/sui-framework/docs"
            >
              Sui framework (GitHub)
            </Link>
            <Link
              className={`${cardlinks} text-md`}
              to="https://github.com/MystenLabs/sui/tree/main/crates/sui-sdk"
            >
              Rust SDK (GitHub)
            </Link>
          </HomeCard>
          <HomeCard title="Resources" aux>
            <Link
              className={`${cardlinks} text-md`}
              to="https://sui.directory/?_project_type=api%2Cdeveloper-tools%2Cinfrastructure%2Csdk"
            >
              Sui ecosystem directory
            </Link>
            <Link className={`${cardlinks} text-md`} to="https://blog.sui.io/">
              Sui blog
            </Link>
            <Link
              className={`${cardlinks} text-md`}
              to="guides/developer/dev-cheat-sheet"
            >
              Sui dev cheat sheet
            </Link>
          </HomeCard>
          <div className={`${darkcardclass} w-[350px] h-[350px]`}>
            <SingleLink to="/guides/developer/first-app"></SingleLink>
          </div>
        </div>

        <div className="lg:w-[1066px] w-auto my-8 md:mt-24 text-white mx-auto text-center">
          <h1 className="mb-4 lg:text-8xl md:text-6xl text-4xl font-normal">
            Why Sui?
          </h1>
          <p className="text-lg sm:text-xl lg:text-3xl mb-0 px-8 md:w-1/2 w-auto mx-auto text-sui-blue-lighter inline-block">
            Sui is the first internet-scale programmable blockchain platform
          </p>
        </div>
        <div className="lg:w-[1066px] width-auto mx-auto flex-none md:flex md:flex-row items-center">
          <div className="text-center mx-auto mb-8 md:mb-0">
            <img
              className="lg:w-[350px] w-[250px]"
              src="/img/index/blocks.png"
              alt="Decorative visual"
            />
          </div>
          <div className="lg:w-[676px] mx-8 md:ml-8 md:mr-0 lg:block sm:block md:hidden">
            <p className="text-xl text-sui-blue-lighter border-solid border-0 border-t border-sui-white/50">
              <span className="inline-block mt-7 mb-3 ml-4">
                Unmatched scalability, instant settlement
              </span>
            </p>
            <p className="text-xl text-sui-blue-lighter border-solid border-0 border-t border-sui-white/50">
              <span className="inline-block mt-7 mb-3 ml-4">
                A safe smart contract language accessible to mainstream
                developers
              </span>
            </p>
            <p className="text-xl text-sui-blue-lighter border-solid border-0 border-t border-sui-white/50">
              <span className="inline-block mt-7 mb-3 ml-4">
                Ability to define rich and composable on-chain assets
              </span>
            </p>
            <p className="text-xl text-sui-blue-lighter border-solid border-0 border-t border-sui-white/50">
              <span className="inline-block mt-7 mb-3 ml-4">
                Better user experience for web3 apps
              </span>
            </p>
          </div>
        </div>
        <div className="sm:w-[840] lg:w-[1066px] w-auto my-12 text-white mx-auto md:block hidden">
          <ContentItem title="Scalability">
            Sui scales horizontally to meet the demands of applications. Network
            capacity grows in proportion to the increase in Sui validators'
            processing power by adding workers, resulting in low gas fees even
            during high network traffic. This scalability characteristic is in
            sharp contrast to other blockchains with rigid bottlenecks.
          </ContentItem>
          <ContentItem title="Move">
            Move design prevents issues such as reentrancy vulnerabilities,
            poison tokens, and spoofed token approvals that attackers have
            leveraged to steal millions on other platforms. The emphasis on
            safety and expressivity provides a more straightforward transition
            from web 2.0 to web3 for developers, without the need to understand
            the intricacies of the underlying infrastructure.
          </ContentItem>
          <ContentItem title="On-chain assets">
            Rich on-chain assets enable new applications and economies based on
            utility without relying solely on artificial scarcity. Developers
            can implement dynamic NFTs that you can upgrade, bundle, and group
            in an application-specific manner, such as changes in avatars and
            customizable items based on gameplay. This capability delivers
            stronger in-game economies as NFT behavior gets fully reflected
            on-chain, making NFTs more valuable and delivering more engaging
            feedback loops.
          </ContentItem>
          <ContentItem title="Built for Web3">
            Sui aims to be the most accessible smart contract platform,
            empowering developers to create great user experiences in web3. To
            usher in the next billion users, Sui empowers developers with
            various tools to take advantage of the power of the Sui blockchain.
            The Sui Development Kit (SDK) will enable developers to build
            without boundaries.
          </ContentItem>
        </div>
      </div>
    </Layout>
  );
}
