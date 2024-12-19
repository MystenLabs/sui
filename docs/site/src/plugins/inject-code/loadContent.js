// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const addCodeInject = async function (content, actions) {
  console.log("LOAD inside load");
  content = JSON.stringify({});
  console.log(content);
  console.log(actions);
};

module.exports = addCodeInject;
