// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React from "react";
import Layout from "@theme/Layout";
import API from "../components/API";

import useDocusaurusContext from "@docusaurus/useDocusaurusContext";

export default function JsonRpc() {
  const { siteConfig } = useDocusaurusContext();
  return (
    <Layout title={`Sui API Reference | ${siteConfig.title}`}>
      <div style={{ maxWidth: '960px', margin: '0 auto', padding: '1rem' }}>
        <p>Complete reference for the Sui JSON-RPC API. Browse available methods, request parameters, and response schemas for interacting with the Sui network programmatically.</p>
      </div>
      <API />
    </Layout>
  );
}
