// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const protocolInject = async function (source) {
  this.cacheable && this.cacheable();

  const callback = this.async();
  const options = this.getOptions();
  const spec = options.protocolSpec;
  console.log(spec.files)
  //const output = `<Protocol toc={${JSON.stringify(toc)}}/>\n${output.join("\n")}`;
  const createId = (name) => {
    return name.replace(/\./g, "-");
  };
  for (const proto of spec.files){
    console.log(proto)
  }
  return callback && callback(null, source);
};

module.exports = protocolInject;
