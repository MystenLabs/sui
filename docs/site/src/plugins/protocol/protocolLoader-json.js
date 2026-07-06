// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const protocolInject = async function (source) {
  this.cacheable && this.cacheable();

  const callback = this.async();
  const options = this.getOptions();

  const spec = JSON.parse(options.protocolSpec);

  // Determine which page we are rendering based on the resource path.
  // fullnode-protocol-messages.mdx → messages page (message definitions + fields)
  // fullnode-protocol-types.mdx    → types page (enums + scalar value types)
  // fullnode-protocol.mdx          → services page (services + file descriptions)
  const isMessagesPage = this.resourcePath.includes("fullnode-protocol-messages");
  const isTypesPage = this.resourcePath.includes("fullnode-protocol-types") || isMessagesPage;

  const toc = [];

  const createId = (name) =>
    String(name ?? "").replace(/[\._]/g, "-").replace(/\//g, "_");

  const suiSorted = (array) =>
    array.sort((a, b) => {
      const aStartsWithSui = a.name.startsWith("sui");
      const bStartsWithSui = b.name.startsWith("sui");
      if (aStartsWithSui && !bStartsWithSui) return -1;
      if (!aStartsWithSui && bStartsWithSui) return 1;
      return 0;
    });

  for (const proto of spec.files) {
    // Skip well-known Google protobuf types on the types page.
    if (isTypesPage && proto.name.startsWith("google/")) continue;

    const messages = [];
    const services = [];
    const enums = [];

    if (proto.messages) {
      for (const message of proto.messages) {
        messages.push({ name: message.name, link: createId(message.fullName) });
      }
    }
    if (proto.services) {
      for (const service of proto.services) {
        services.push({ name: service.name, link: createId(service.fullName) });
      }
    }
    if (proto.enums) {
      for (const num of proto.enums) {
        enums.push({ name: num.name, link: createId(num.fullName) });
      }
    }

    // Filter TOC entries based on which page we are rendering.
    // Always include all three arrays so the Protocol component can safely
    // access .messages.length, .services.length, and .enums.length.
    if (isMessagesPage) {
      // Messages page shows messages only.
      if (messages.length > 0) {
        toc.push({
          name: proto.name,
          link: createId(proto.name),
          messages,
          services: [],
          enums: [],
        });
      }
    } else if (isTypesPage) {
      // Types page shows enums only (messages are on the messages page).
      if (enums.length > 0) {
        toc.push({
          name: proto.name,
          link: createId(proto.name),
          messages: [],
          services: [],
          enums,
        });
      }
    } else {
      // Services page shows services only.
      if (services.length > 0) {
        toc.push({
          name: proto.name,
          link: createId(proto.name),
          messages: [],
          services,
          enums: [],
        });
      }
    }
  }

  const types = [];
  if (isTypesPage && !isMessagesPage) {
    for (const prototype of spec.scalarValueTypes) {
      types.push({ name: prototype.protoType, link: prototype.protoType });
    }
  }

  const tocSorted = suiSorted(toc);
  if (isTypesPage && !isMessagesPage && types.length > 0) {
    tocSorted.push({
      name: "Scalar Value Types",
      link: "scalar-value-types",
      messages: types,
      services: [],
      enums: [],
    });
  }

  // Unescaped curly braces mess up docusaurus render
  const handleCurlies = (text) => {
    let isCodeblock = false;

    const final = String(text ?? "").split("\n");
    for (const [idx, line] of final.entries()) {
      if (line.includes("```")) isCodeblock = !isCodeblock;

      if (!isCodeblock) {
        const curlyIndices = [];
        let insideBackticks = false;

        for (let i = 0; i < line.length; i++) {
          if (line[i] === "`") insideBackticks = !insideBackticks;
          if (line[i] === "{" && !insideBackticks) curlyIndices.unshift(i);
        }

        for (const j of curlyIndices) {
          final[idx] = [
            line.substring(0, j),
            "&#123;",
            line.substring(j + 1),
          ].join("");
        }
      }
    }

    return final.join("\n");
  };

  const hasText = (v) => String(v ?? "").trim().length > 0;

  const content = [`<Protocol toc={${JSON.stringify(tocSorted)}}/>`];

  const messageSort = (array) =>
    array.sort((a, b) => a.name.localeCompare(b.name));

  const files = suiSorted(spec.files);

  // -----------------------------
  // CSS classes (custom.css owns styling)
  // -----------------------------
  const moduleCard = "protoCard";
  const moduleHeader = "protoCardHeader";
  const moduleDesc = "protoCardDesc";

  const tableWrap = "protoGrid";
  const tableSectionTitle = "protoGridTitle";

  const cellKey = "protoKey";
  const cellVal = "protoVal";
  const valMuted = "protoValMuted";

  const badgeBase = "protoBadge";
  const badgeOptional = "protoBadge protoBadgeOptional";
  const badgeRepeated = "protoBadge protoBadgeRepeated";

  const prose = "protoProse"; // optional; define in CSS if you want

  for (const file of files) {
    // Determine if this file has content for the current page.
    const hasServices = file.services.length > 0;
    const hasMessages = file.messages.length > 0;
    const hasEnums = file.enums.length > 0;

    if (isMessagesPage && !hasMessages) continue;
    if (isTypesPage && !isMessagesPage && !hasEnums) continue;
    if (!isTypesPage && !hasServices) continue;

    // Skip well-known Google protobuf types on the types page to reduce
    // page size.  These are standard types documented at protobuf.dev.
    if (isTypesPage && file.name.startsWith("google/")) continue;

    content.push(`\n## ${file.name} {#${createId(file.name)}}`);

    if (hasText(file.description)) {
      content.push(
        `<div class="${prose}">\n${handleCurlies(file.description).replace(
          /\n##? (.*)\n/,
          "### $1",
        )}\n</div>`,
      );
    }

    // Messages (messages page only)
    if (isMessagesPage && file.messages.length > 0) {

      for (const message of file.messages) {
        let fields = [];
        let oneofFields = [];
        const declarations = [];

        if (message.hasOneofs) {
          for (const field of message.fields) {
            if (field.isoneof === true && !field.oneofdecl.match(/^_/)) {
              oneofFields.push(field);
              if (!declarations.includes(field.oneofdecl)) {
                declarations.push(field.oneofdecl);
              }
            } else {
              fields.push(field);
            }
          }
        } else {
          fields = Object.values(message.fields);
        }

        fields = messageSort(fields);
        oneofFields = messageSort(oneofFields);

        content.push(`<div class="${moduleCard}">`);
        content.push(
          `<div class="${moduleHeader}" id="${createId(message.fullName)}">${message.name}</div>`,
        );

        if (hasText(message.description)) {
          let desc = handleCurlies(message.description)
            .replace(/</g, "&#lt;")
            .replace(/^(#{1,2})\s(?!#)/gm, "### ");
          // Truncate long descriptions to first paragraph
          const paraEnd = desc.indexOf('\n\n');
          if (paraEnd > 0 && desc.length > 200) {
            desc = desc.slice(0, paraEnd);
          }
          content.push(
            `<div class="${moduleDesc}">\n${desc}\n</div>`,
          );
        }

        const MAX_FIELDS = 4;
        const allFields = [...fields, ...oneofFields];

        if (allFields.length > 0) {
          content.push(`<div class="${tableWrap}">`);
        }

        const displayFields = fields.slice(0, MAX_FIELDS);
        for (const f of displayFields) {
          const typePart = (f.type && f.type !== "") ? ` <a href="#${createId(f.fullType)}">${f.type}</a>` : '';
          const labelPart = f.label === 'optional' ? ' *(optional)*' : f.label === 'repeated' ? ' *(repeated)*' : '';
          const descPart = hasText(f.description) ? ` — ${handleCurlies(f.description).replace(/\n\/?/g, ' ').replace(/<(http.*)>/g, '$1')}` : '';
          content.push(`<div class="${cellKey}">${f.name}</div><div class="${cellVal}">${typePart}${labelPart}${descPart}</div>`);
        }

        if (fields.length > MAX_FIELDS) {
          const remaining = fields.length - MAX_FIELDS + oneofFields.length;
          content.push(`<div class="protoTruncNote" style={{gridColumn:"1/-1"}}>... and ${remaining} more fields. <a href="/doc/protocol-messages-full.html#${createId(message.fullName)}">View all fields</a></div>`);
        } else {
          for (const declaration of declarations) {
            content.push(`<div class="protoUnionNote" style={{gridColumn:"1/-1"}}>Union: <b>${declaration}</b></div>`);
            for (const f of oneofFields) {
              if (f.oneofdecl !== declaration) continue;
              const typePart = (f.type && f.type !== "") ? ` <a href="#${createId(f.fullType)}">${f.type}</a>` : '';
              const descPart = hasText(f.description) ? ` — ${handleCurlies(f.description).replace(/\n\/?/g, ' ').replace(/<(http.*)>/g, '$1')}` : '';
              content.push(`<div class="${cellKey}">${f.name}</div><div class="${cellVal}">${typePart}${descPart}</div>`);
            }
          }
        }

        if (allFields.length > 0) {
          content.push(`</div>`);
        }

        content.push(`</div>`); // protoCard
      }
    }

    // Services (services page only)
    if (!isTypesPage && file.services.length > 0) {
      const proto = file.name.split("/").pop();
      content.push(`<div class="protoSpacer"></div>`);
      content.push(`### Services (${proto})`);

      for (const service of file.services) {
        content.push(`<div class="${moduleCard}">`);
        content.push(
          `<div class="${moduleHeader}" id="${createId(
            service.fullName,
          )}">${service.name}</div>`,
        );

        if (hasText(service.description)) {
          content.push(
            `<div class="${moduleDesc}">${handleCurlies(service.description)
              .replace(/\n\/?/g, " ")
              .replace(/<(http.*)>/g, "$1")}</div>`,
          );
        }

        if (service.methods && service.methods.length > 0) {
          content.push(`<div class="${tableWrap}">`);
          content.push(`<div class="${tableSectionTitle}">Methods</div>`);

          for (const method of service.methods) {
            content.push(
              `<div class="${cellKey} protoMethodSig">` +
              `<span class="protoMethodName">${method.name}</span>` +
              `<div class="protoMethodTypes">` +
              `<a href="fullnode-protocol-messages#${createId(method.requestFullType)}">${method.requestType}</a>` +
              `<span class="protoArrow">&#8594;</span>` +
              `<a href="fullnode-protocol-messages#${createId(method.responseFullType)}">${method.responseType}</a>` +
              `</div>` +
              `</div>`,
            );

            content.push(`<div class="${cellVal}">`);
            if (hasText(method.description)) {
              content.push(
                `<div class="${valMuted}">${handleCurlies(method.description)
                  .replace(/\n\/?/g, " ")
                  .replace(/<(http.*)>/g, "$1")}</div>`,
              );
            }
            content.push(`</div>`);
          }

          content.push(`</div>`);
        }

        content.push(`</div>`);
      }
    }

    // Enums (types page only — not messages page)
    if (isTypesPage && !isMessagesPage && file.enums.length > 0) {
      content.push(`<h4 class="protoSectionTitle">Enums</h4>`);

      for (const num of file.enums) {
        content.push(`<div class="${moduleCard}">`);
        content.push(
          `<div class="${moduleHeader}" id="${createId(num.fullName)}">${num.name}</div>`,
        );

        if (hasText(num.description)) {
          content.push(
            `<div class="${moduleDesc}">${handleCurlies(num.description).replace(/\n\/?/g, ' ').replace(/<(http.*)>/g, '$1')}</div>`,
          );
        }

        if (num.values && num.values.length > 0) {
          content.push(`<div class="${tableWrap}">`);
          content.push(`<div class="${tableSectionTitle}">Values</div>`);
          for (const val of num.values) {
            content.push(`<div class="${cellKey}"><code>${val.name}</code></div>`);
            content.push(`<div class="${cellVal}">`);
            if (hasText(val.description)) {
              content.push(`<span class="${valMuted}">${handleCurlies(val.description).replace(/\n+\/?/g, ' ').replace(/<(http.*)>/g, '$1')}</span>`);
            }
            content.push(`</div>`);
          }
          content.push(`</div>`);
        }

        content.push(`</div>`); // protoCard
      }
    }
  }

  // Scalar Value Types (types page only, not messages page)
  if (isTypesPage && !isMessagesPage) {
    content.push("\n## Scalar Value Types {#scalar-value-types}");
    content.push("");
    content.push("| Proto Type | C++ | Go | Java | Python | Notes |");
    content.push("|---|---|---|---|---|---|");

    for (const scalar of spec.scalarValueTypes) {
      const notes = hasText(scalar.notes) ? handleCurlies(scalar.notes).replace(/\n/g, ' ').replace(/\|/g, '\\|') : '';
      content.push(
        `| **${scalar.protoType}** | ${scalar.cppType} | ${scalar.goType} | ${scalar.javaType} | ${scalar.pythonType} | ${notes} |`,
      );
    }
  }

  return (
    callback &&
    callback(null, source.replace(/<Protocol ?\/>/, content.join("\n")))
  );
};

module.exports = protocolInject;
