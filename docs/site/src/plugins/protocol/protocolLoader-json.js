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

      if (message.fields.length > 0) {
        const valStyle =
          "border-l-2 pl-4 border-solid border-transparent border-l-sui-gray-65 col-span-10";
        const fieldAttr = "pr-4 mr-[2px] col-span-2";
        content.push(`<p class="ml-4 text-xl">Fields</p>\n<div class="ml-4">`);
        content.push(
          `<div class="border border-solid border-y-transparent border-r-transparent border-l-sui-gray-65">`,
        );
        for (const field of message.fields) {
          const hasType = field.type && field.type !== "";
          const hasLabel = field.label && field.label !== "";
          const hasDesc = field.description && field.description !== "";
          content.push(`<p class="relative inline-flex items-center ml-4 mb-0 before:content-[''] before:absolute before:left-[-1rem] 
            before:border-t-transparent before:border-b-transparent before:border-solid
            before:border-y-5 before:border-r-0 before:border-l-8 before:border-l-sui-gray-65"><span class="text-xl">${field.name}</span></p>`);
          content.push(
            `<div class="grid grid-cols-12 items-center gap-2 my-2 p-4 bg-sui-ghost-white dark:bg-sui-ghost-dark border border-solid dark:border-sui-gray-95 border-sui-gray-50">`,
          );

          if (hasType) {
            content.push(`<div class="${fieldAttr} text-right">Type</div>`);
            content.push(
              `<div class="${valStyle}">[${field.type}](#${createId(field.fullType)})</div>`,
            );
          }
          if (hasLabel) {
            content.push(`<div class="${fieldAttr} text-right">Label</div>`);
            content.push(`<div class="${valStyle}">${field.label}</div>`);
          }
          if (hasDesc) {
            content.push(
              `<div class="${fieldAttr} text-right">Description</div>`,
            );
            content.push(
              `<div class="${valStyle}">${field.description
                .replace(/{/g, "&#123;")
                .replace(/\n\/?/g, "")
                .replace(/<(http.*)>/g, "$1")}</div>`,
            );
          }
          content.push(`</div>`);
        }
        content.push("</div>\n</div>");
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
