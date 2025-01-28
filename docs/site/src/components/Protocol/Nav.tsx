// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import React, { useState } from "react";

export default function Protocol(props) {
  const { toc } = props;
  const [proto, setProto] = useState(toc[0]);
  const [methods, setMethods] = useState(toc[0].items);
  if (!toc) {
    return;
  }

  const handleProtoChange = (e) => {
    console.log(e.target.value);
    console.log(toc[proto]);
    const selected = e.target.value;
    setProto(selected);
    setMethods(toc.filter((t) => t.title === selected)[0].items || []);
  };

  return (
    <div>
      <select onChange={handleProtoChange}>
        {toc.map((item) => {
          return <option value={item.title}>{item.title}</option>;
        })}
      </select>
      <select>
        {methods.map((method) => {
          return <option value={method.title}>{method.title}</option>;
        })}
      </select>
    </div>
  );
}
