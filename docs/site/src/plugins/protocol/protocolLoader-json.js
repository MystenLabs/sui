// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const protocolInject = async function (source) {
  this.cacheable && this.cacheable();

  const callback = this.async();
  const options = this.getOptions();
  const spec = JSON.parse(options.protocolSpec);
  const toc = [];
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
      const cellStyle = "p-4 border border-solid align-center";
      if (message.fields.length > 0) {
        content.push(
          `\n<table class="w-full table table-fixed">\n<thead>\n<tr>\n<th class="${cellStyle} w-[21%]">Field</th><th class="${cellStyle} w-[23%]">Type</th><th class="${cellStyle} w-[9%] whitespace-nowrap truncate">Label</th><th class="${cellStyle} w-[47%]">Description</th>\n</tr>\n</thead>\n<tbody>`,
        );

        for (const field of message.fields) {
          content.push(
            `<tr>\n<td class="${cellStyle} whitespace-nowrap overflow-auto">${field.name}</td><td class="${cellStyle} whitespace-nowrap overflow-auto">[${field.type}](#${createId(field.fullType)})</td><td class="${cellStyle} whitespace-nowrap truncate">${field.label}</td><td class="${cellStyle}">${field.description
              .replace(/{/g, "&#123;")
              .replace(/\n\/?/g, "")
              .replace(/<(http.*)>/g, "$1")}</td>\n</tr>`,
          );
        }
        content.push(`</tbody>\n</table>\n`);
      }
    }
  }

  content.push("\n## Scalar Value Types");
  const cellStyle = "p-4 border border-solid align-center text-center";
  for (const scalar of spec.scalarValueTypes) {
    content.push(`\n### ${scalar.protoType}`);
    content.push(`${scalar.notes.replace(/{/g, "&#123;")}`);
    content.push(
      `\n<table class="w-full table table-fixed">\n<thead>\n<tr>\n<th class="${cellStyle}">C++</th><th class="${cellStyle}">Java</th><th class="${cellStyle}">Python</th><th class="${cellStyle}">Go</th><th class="${cellStyle}">C#</th><th class="${cellStyle}">PHP</th><th class="${cellStyle}">Ruby</th>\n</tr>\n</thead>\n<tbody>`,
    );
    content.push(
      `<tr>\n<td class="${cellStyle}">${scalar.cppType}</td><td class="${cellStyle}">${scalar.javaType}</td><td class="${cellStyle}">${scalar.pythonType}</td><td class="${cellStyle}">${scalar.goType}</td><td class="${cellStyle}">${scalar.csType}</td><td class="${cellStyle}">${scalar.phpType}</td><td class="${cellStyle}">${scalar.rubyType}</td>\n</tr>`,
    );
    content.push(`</tbody>\n</table>\n`);
  }

  return (
    callback &&
    callback(null, source.replace(/<Protocol ?\/>/, content.join(`\n`)))
  );
};

module.exports = protocolInject;
