// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const protocolInject = async function (source) {
  this.cacheable && this.cacheable();

  const callback = this.async();
  const options = this.getOptions();
  const spec = JSON.parse(options.protocolSpec);
  const toc = [];

  //const output = `<Protocol toc={${JSON.stringify(toc)}}/>\n${output.join("\n")}`;
  const createId = (name) => {
    return name.replace(/[\._]/g, "-").replace(/\//g, "_");
  };
  for (const proto of spec.files) {
    let messages = [];
    let protoLink = createId(proto.name);
    //let protoLinkPre = protoLink.replace(/-proto$/, "");
    if (proto.messages) {
      for (const message of proto.messages) {
        messages.push({
          name: message.name,
          link: createId(message.fullName),
        });
      }
    }
    let item = { name: proto.name, link: protoLink, messages: messages };
    toc.push(item);
  }
  const types = [];
  for (const prototype of spec.scalarValueTypes) {
    types.push({ name: prototype.protoType, link: prototype.protoType });
  }
  toc.push({
    name: "Scalar Value Types",
    link: "scalar-value-types",
    messages: types,
  });

  let content = [`<Protocol toc={${JSON.stringify(toc)}}/>`];
  for (const file of spec.files) {
    content.push(`\n## ${file.name} {#${createId(file.name)}}`);
    content.push(
      `${file.description.replace(/{/g, "&#123;").replace(/^##? (.*)\n/, "### $1")}`,
    );
    for (const message of file.messages) {
      content.push(`\n### ${message.name} {#${createId(message.fullName)}}`);
      content.push(
        `${message.description
          .replace(/{/g, "&#123;")
          .replace(/</g, "&#lt;")
          .replace(/^(#{1,2})\s(?!#)/gm, "### ")}`,
      );
      if (message.fields.length > 0) {
        content.push("| Field | Type | Label | Description |");
        content.push("|---|---|---|---|");

        for (const field of message.fields) {
          content.push(
            `| ${field.name} | [${field.type}](#${createId(field.fullType)}) | ${field.label} | ${field.description
              .replace(/{/g, "&#123;")
              .replace(/\n\/?/g, "")
              .replace(/<(http.*)>/g, "$1")} |`,
          );
        }
      }
    }
  }
  content.push("\n## Scalar Value Types");
  for (const scalar of spec.scalarValueTypes) {
    content.push(`\n### ${scalar.protoType}`);
    content.push(`${scalar.notes.replace(/{/g, "&#123;")}`);
    content.push("| C++ | Java | Python | Go | C# | PHP | Ruby |");
    content.push("|---|---|---|---|---|---|---|");
    content.push(
      `|${scalar.cppType}|${scalar.javaType}|${scalar.pythonType}|${scalar.goType}|${scalar.csType}|${scalar.phpType}|${scalar.rubyType}|`,
    );
  }

  return (
    callback &&
    callback(null, source.replace(/<Protocol ?\/>/, content.join(`\n`)))
  );
};

module.exports = protocolInject;
