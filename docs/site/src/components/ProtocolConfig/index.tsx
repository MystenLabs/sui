// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React, { useState, useEffect } from "react";
import Tabs from "@theme/Tabs";
import TabItem from "@theme/TabItem";
import axios from "axios";

export default function ProtocolConfig(props) {
  const data = {
    jsonrpc: "2.0",
    id: 1,
    method: "sui_getProtocolConfig",
    params: [],
  };
  const urls = [
    "https://fullnode.mainnet.sui.io:443",
    "https://fullnode.testnet.sui.io:443",
    "https://fullnode.devnet.sui.io:443",
  ];
  const [results, setResults] = useState({
    mainnet: null,
    testnet: null,
    devnet: null,
  });
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(null);
  const { fields } = props;

  const parseResult = (data) => {
    let items = Object.entries(data);

    return items.map((item) => {
      if (item[1] === null) {
        return item;
      }
      if (typeof item[1] === "object") {
        const [k, v] = Object.entries(item[1])[0];
        return [item[0], k, v];
      }
      return item;
    });
  };

  const DisplayResults = (props) => {
    const { results } = props;
    return (
      <table className="table w-full">
        <thead>
          <tr>
            <th>Parameter</th>
            <th>Type</th>
            <th>Value</th>
          </tr>
        </thead>
        <tbody>
          {results.map((item, index) => (
            <>
              {(!fields || fields.includes(item[0])) && (
                <tr key={index}>
                  <td>{item[0]}</td>
                  <td>{item[1]}</td>
                  <td>{item[2] ? item[2] : "null"}</td>
                </tr>
              )}
            </>
          ))}
        </tbody>
      </table>
    );
  };

  useEffect(() => {
    const fetchData = async () => {
      try {
        const responses = await Promise.all(
          urls.map((url) =>
            axios.post(url, data, {
              headers: {
                "Content-Type": "application/json",
              },
            }),
          ),
        );

        setResults({
          mainnet: parseResult(responses[0].data.result.attributes),
          testnet: parseResult(responses[1].data.result.attributes),
          devnet: parseResult(responses[2].data.result.attributes),
        });
      } catch (err) {
        setError(err.message);
      } finally {
        setLoading(false);
      }
    };

    fetchData();
  }, []);

  if (loading) {
    return <div>Loading...</div>;
  }

  if (error) {
    return <div>Error: {error}</div>;
  }

  return (
    <Tabs groupId="sui-network">
      <TabItem value="mainnet" label="Mainnet">
        <DisplayResults results={results.mainnet} />
      </TabItem>
      <TabItem value="testnet" label="Testnet">
        <DisplayResults results={results.testnet} />
      </TabItem>
      <TabItem value="devnet" label="Devnet">
        <DisplayResults results={results.devnet} />
      </TabItem>
    </Tabs>
  );
}
