// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const protocolInject = async function (source) {
  this.cacheable && this.cacheable();

  const callback = this.async();
  const options = this.getOptions();

  const spec = JSON.parse(options.protocolSpec);
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

    toc.push({
      name: proto.name,
      link: createId(proto.name),
      messages,
      services,
      enums,
    });
  }

  const types = [];
  for (const prototype of spec.scalarValueTypes) {
    types.push({ name: prototype.protoType, link: prototype.protoType });
  }

  const tocSorted = suiSorted(toc);
  tocSorted.push({
    name: "Scalar Value Types",
    link: "scalar-value-types",
    messages: types,
  });

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
    content.push(`\n## ${file.name} {#${createId(file.name)}}`);

    if (hasText(file.description)) {
      content.push(
        `<div class="${prose}">\n${handleCurlies(file.description).replace(
          /\n##? (.*)\n/,
          "### $1",
        )}\n</div>`,
      );
    }

    content.push(`<div class="protoIndent">`);

    if (file.messages.length > 0) {
      content.push(`<h4 class="protoSectionTitle">Messages</h4>`);
    }

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
        `<div class="${moduleHeader}" id="${createId(
          message.fullName,
        )}">${message.name}</div>`,
      );

      if (hasText(message.description)) {
        content.push(
          `<div class="${moduleDesc}">\n${handleCurlies(message.description)
            .replace(/</g, "&#lt;")
            .replace(/^(#{1,2})\s(?!#)/gm, "### ")}\n</div>`,
        );
      }

      if (fields.length > 0 || oneofFields.length > 0) {
        content.push(`<div class="${tableWrap}">`);
        content.push(`<div class="${tableSectionTitle}">Fields</div>`);
      }

      // Standard fields
      for (const f of fields) {
        const hasType = f.type && f.type !== "";
        const hasLabel = f.label && f.label !== "";
        const hasDesc = hasText(f.description);

        content.push(`<div class="${cellKey}">${f.name}</div>`);
        content.push(`<div class="${cellVal}">`);

        if (hasType) {
          content.push(
            `<div><a href="#${createId(f.fullType)}">${f.type}</a></div>`,
          );
        }

        if (hasLabel) {
          let label = f.label[0].toUpperCase() + f.label.substring(1);
          let badgeClass = badgeBase;

          if (f.label === "optional") {
            label = "Proto3 optional";
            badgeClass = badgeOptional;
          } else if (f.label === "repeated") {
            label = "Repeated []";
            badgeClass = badgeRepeated;
          }

          content.push(`<div class="${badgeClass}">${label}</div>`);
        }

        if (hasDesc) {
          content.push(
            `<div class="${valMuted}">${handleCurlies(f.description)
              .replace(/\n\/?/g, " ")
              .replace(/<(http.*)>/g, "$1")}</div>`,
          );
        }

        content.push(`</div>`);
      }

      // Oneof blocks
      for (const declaration of declarations) {
        content.push(
          `<div class="protoUnionNote">Union field <b>${declaration}</b> can be only one of the following.</div>`,
        );

        for (const f of oneofFields) {
          if (f.oneofdecl !== declaration) continue;

          const hasType = f.type && f.type !== "";
          const hasLabel = f.label && f.label !== "";
          const hasDesc = hasText(f.description);

          content.push(`<div class="${cellKey}">${f.name}</div>`);
          content.push(`<div class="${cellVal}">`);

          if (hasType) {
            content.push(
              `<div><a href="#${createId(f.fullType)}">${f.type}</a></div>`,
            );
          }

          if (hasLabel) {
            content.push(`<div class="${valMuted}">${f.label}</div>`);
          }

          if (hasDesc) {
            content.push(
              `<div class="${valMuted}">${handleCurlies(f.description)
                .replace(/\n\/?/g, " ")
                .replace(/<(http.*)>/g, "$1")}</div>`,
            );
          }

          content.push(`</div>`);
        }
      }

      if (fields.length > 0 || oneofFields.length > 0) {
        content.push(`</div>`); // tableWrap
      }

      content.push(`</div>`); // moduleCard
    }

    // Services
    if (file.services.length > 0) {
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
            // Key cell: method signature (request → response), nowrap so it
            // never breaks mid-type-name. Matches the cellKey/cellVal pattern
            // used by fields so the grid columns align correctly.
            content.push(
              `<div class="${cellKey} protoMethodSig">` +
              `<a href="#${createId(method.requestFullType)}">${method.requestType}</a>` +
              `<span class="protoArrow">&#8594;</span>` +
              `<span>${method.responseType}</span>` +
              `</div>`,
            );

            // Val cell: description (empty div keeps grid alignment if no desc)
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

    // Enums
    if (file.enums.length > 0) {
      content.push(`<h4 class="protoSectionTitle">Enums</h4>`);

      for (const num of file.enums) {
        content.push(`<div class="${moduleCard}">`);
        content.push(
          `<div class="${moduleHeader}" id="${createId(
            num.fullName,
          )}">${num.name}</div>`,
        );

        if (hasText(num.description)) {
          content.push(
            `<div class="${moduleDesc}">${handleCurlies(num.description)
              .replace(/\n\/?/g, " ")
              .replace(/<(http.*)>/g, "$1")}</div>`,
          );
        }

        if (num.values && num.values.length > 0) {
          content.push(`<div class="${tableWrap}">`);
          content.push(`<div class="${tableSectionTitle}">Values</div>`);

          for (const val of num.values) {
            content.push(`<div class="${cellKey}"><code>${val.name}</code></div>`);
            content.push(`<div class="${cellVal}">`);
            if (hasText(val.description)) {
              content.push(
                `<div class="${valMuted}">${handleCurlies(val.description)
                  .replace(/\n+\/?/g, " ")
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

    content.push(`</div>`); // indent
  }

  // Scalar Value Types
  content.push("\n## Scalar Value Types");

  for (const scalar of spec.scalarValueTypes) {
    content.push(`\n### ${scalar.protoType}`);
    if (hasText(scalar.notes)) {
      content.push(`<div class="${prose}">\n${handleCurlies(scalar.notes)}\n</div>`);
    }

    content.push(`<div class="protoScalarRow">`);
    content.push(
      `<div class="protoScalarCard"><div class="protoScalarTitle">C++</div><div class="protoScalarVal">${scalar.cppType}</div></div>`,
    );
    content.push(
      `<div class="protoScalarCard"><div class="protoScalarTitle">C#</div><div class="protoScalarVal">${scalar.csType}</div></div>`,
    );
    content.push(
      `<div class="protoScalarCard"><div class="protoScalarTitle">Go</div><div class="protoScalarVal">${scalar.goType}</div></div>`,
    );
    content.push(
      `<div class="protoScalarCard"><div class="protoScalarTitle">Java</div><div class="protoScalarVal">${scalar.javaType}</div></div>`,
    );
    content.push(
      `<div class="protoScalarCard"><div class="protoScalarTitle">PHP</div><div class="protoScalarVal">${scalar.phpType}</div></div>`,
    );
    content.push(
      `<div class="protoScalarCard"><div class="protoScalarTitle">Python</div><div class="protoScalarVal">${scalar.pythonType}</div></div>`,
    );
    content.push(
      `<div class="protoScalarCard"><div class="protoScalarTitle">Ruby</div><div class="protoScalarVal">${scalar.rubyType}</div></div>`,
    );
    content.push(`</div>`);
  }

  return (
    callback &&
    callback(null, source.replace(/<Protocol ?\/>/, content.join("\n")))
  );
};

module.exports = protocolInject;