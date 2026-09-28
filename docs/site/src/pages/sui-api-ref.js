// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React from "react";
import Layout from "@theme/Layout";
import API from "../components/API";

import useDocusaurusContext from "@docusaurus/useDocusaurusContext";

export default function JsonRpc() {
  const { siteConfig } = useDocusaurusContext();
  return (
    <Layout title={`Sui API Reference (Legacy) | ${siteConfig.title}`}>
      <div style={{ maxWidth: "960px", margin: "0 auto", padding: "1rem" }}>
        <div className="legacy-api-banner" role="alert">
          <p className="legacy-api-banner__title">
            Legacy API scheduled for removal
          </p>
          <p>
            Sui Foundation disabled JSON-RPC on Mainnet full nodes the week of
            July&nbsp;27,&nbsp;2026, and plans full decommission, including code
            removal, for mid-October&nbsp;2026, when this reference moves to the
            archive. Use this page to identify the legacy methods your
            application still calls, not to build new integrations. Call{" "}
            <a href="/develop/accessing-data/grpc">gRPC</a> or{" "}
            <a href="/develop/accessing-data/graphql/graphql-rpc">
              GraphQL RPC
            </a>{" "}
            instead, and see the{" "}
            <a href="/develop/accessing-data/json-rpc-migration">
              JSON-RPC Migration Guide
            </a>{" "}
            for the method-by-method mapping.
          </p>
        </div>
        <p>
          Complete reference for the legacy Sui JSON-RPC API. Browse methods,
          request parameters, and response schemas to identify the calls your
          application still depends on, then map each one to its gRPC or GraphQL
          replacement.
        </p>
      </div>
      <API />
    </Layout>
  );
}
