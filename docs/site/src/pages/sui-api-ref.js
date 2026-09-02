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
            JSON-RPC was disabled on Sui Foundation Mainnet full nodes the week
            of July&nbsp;27,&nbsp;2026. Full decommission, including code
            removal, is planned for mid-October&nbsp;2026, after which this
            reference is archived. This page is kept for identifying legacy
            methods during migration. Do not build new integrations against it.
            Use <a href="/develop/accessing-data/grpc">gRPC</a> or{" "}
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
