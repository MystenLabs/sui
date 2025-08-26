// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// This code replaces the @docusaurus-plugin-include community plugin
// it handles {@include: } and {@inject: } embeds
// For inject, it uses the same logic as the custom inject-code plugin
// and supports #fun, #var, #struct selection.

// This code also does some special formatting
// so that pages with HTML content like the auto-gen pages
// get rendered properly
// and that any page with markdownX elements get rendered as such

const fs = require("fs");
const path = require("path");
const https = require("https");
const utils = require("./utils.js");

const { visitParents } = require("unist-util-visit-parents");
const { fromMarkdown } = require("mdast-util-from-markdown");
const { mdxFromMarkdown } = require("mdast-util-mdx");
const { mdxjs } = require("micromark-extension-mdxjs");
const { gfmFromMarkdown } = require("mdast-util-gfm");

// Markdown -> HTML for heavy pages
const { micromark } = require("micromark");
const { gfm, gfmHtml } = require("micromark-extension-gfm");
const { directive } = require("micromark-extension-directive");
const { directiveFromMarkdown } = require("mdast-util-directive");

// Directories with heavy Markdown/HTML that should bypass normal MDX parsing
function heavyHtmlDirs(docsDir) {
  return [
    path.resolve(docsDir, "references", "framework"),
    path.resolve(docsDir, "content", "references", "framework"),
  ];
}

// ---------- Helpers ----------

function fixAngleBracketAutolinksInHtml(html) {
  // Turn <https://…> (and &lt;https://…&gt;) into clickable anchors.
  // Preserve a trailing punctuation char if present.
  return html
    .replace(
      /<(https?:\/\/[^\s<>")]+)>([.,;:!?)]?)/g,
      '<a href="$1" target="_blank" rel="noopener noreferrer nofollow">$1</a>$2',
    )
    .replace(
      /&lt;(https?:\/\/[^\s<>"\')]+)&gt;([.,;:!?)]?)/g,
      '<a href="$1" target="_blank" rel="noopener noreferrer nofollow">$1</a>$2',
    );
}

function containsAdmonition(md) {
  return /^:::(?:info|note|tip|warning|danger|caution)(?:\s|$)/m.test(md);
}

function isInsideDir(filePathAbs, dirAbs) {
  const rel = path.relative(dirAbs, filePathAbs);
  return rel === "" || (!rel.startsWith("..") && !path.isAbsolute(rel));
}

function parseMarkdownToNodes(markdownText) {
  const tree = fromMarkdown(markdownText, {
    extensions: [mdxjs(), gfm(), directive()],
    mdastExtensions: [
      mdxFromMarkdown(),
      gfmFromMarkdown(),
      directiveFromMarkdown(),
    ],
  });
  return tree.children || [];
}

// Strip leading YAML front-matter (avoid re-injecting it into the HTML)
function stripYamlFrontMatter(text) {
  return text.replace(/^\uFEFF?---\s*\r?\n[\s\S]*?\r?\n---\s*\r?\n?/, "");
}

// Convert Markdown to HTML with GFM so links/headings/tables render correctly
function markdownToHtml(md) {
  return micromark(md, {
    allowDangerousHtml: true,
    extensions: [gfm()],
    htmlExtensions: [gfmHtml()],
  });
}

// Build an mdxJsxFlowElement:
//   <div className="markdown" dangerouslySetInnerHTML={{__html: "<html…>"}} />
function jsxDivWithInlineHtml(html) {
  const rawLiteral = JSON.stringify(html); // safe JS string for the attribute

  // ESTree representing: ({ __html: "<html…>" })
  const estree = {
    type: "Program",
    sourceType: "module",
    body: [
      {
        type: "ExpressionStatement",
        expression: {
          type: "ObjectExpression",
          properties: [
            {
              type: "Property",
              key: { type: "Identifier", name: "__html" },
              value: { type: "Literal", value: html, raw: rawLiteral },
              kind: "init",
              method: false,
              shorthand: false,
              computed: false,
            },
          ],
        },
      },
    ],
  };

  return {
    type: "mdxJsxFlowElement",
    name: "div",
    attributes: [
      { type: "mdxJsxAttribute", name: "className", value: "markdown" },
      {
        type: "mdxJsxAttribute",
        name: "dangerouslySetInnerHTML",
        value: {
          type: "mdxJsxAttributeValueExpression",
          value: `({__html: ${rawLiteral}})`,
          data: { estree },
        },
      },
    ],
    children: [],
  };
}

// --- Framework doc normalization (fix headings/anchors before HTML conversion) ---

// Turn "## Title {#anchor-id}" into <h2 id="anchor-id">Title</h2> (any heading level)
function convertKramdownHeadingIds(md) {
  return md.replace(
    /^(\s{0,3})(#{1,6})\s+(.+?)\s*\{#([A-Za-z0-9_.:\-]+)\}\s*$/gm,
    (_, indent, hashes, content, id) => {
      const level = hashes.length;
      return `${indent}<h${level} id="${id}">${content}</h${level}>`;
    },
  );
}

// Some cargo-doc pages have "<a name="id"></a>" then a heading line with hashes
function convertNamedAnchorHeadingPairs(md) {
  let out = md.replace(
    /<a\s+name="([^"]+)"\s*><\/a>\s*\n+\s*(#{1,6})\s+([^\n]+?)\s*$/gm,
    (_, id, hashes, content) => {
      const level = hashes.length;
      return `<h${level} id="${id}">${content}</h${level}>`;
    },
  );
  // Fallback: anchor + plain title (no hashes) -> default to <h3>
  out = out.replace(
    /<a\s+name="([^"]+)"\s*><\/a>\s*\n+\s*(?!(?:[*-]|\d+\.)\s|#{1,6}\s|<|```)\s*([^\n{<][^{}\n]*?)\s*$/gm,
    (_, id, content) => `<h3 id="${id}">${content}</h3>`,
  );
  return out;
}

// Some pages use "Title {#id}" with no leading hashes. Treat as <h3> by default.
function convertLooseKramdownIds(md) {
  return md.replace(
    /^(\s{0,3})(?!(?:[*-]|\d+\.)\s)(?!<)(?!#{1,6}\s)([^\n{<][^{}\n]*?)\s*\{#([A-Za-z0-9_.:\-]+)\}\s*$/gm,
    (_, indent, text, id) => `${indent}<h3 id="${id}">${text}</h3>`,
  );
}

function normalizeFrameworkMarkdown(md) {
  let out = stripYamlFrontMatter(md);
  out = convertNamedAnchorHeadingPairs(out);
  out = convertKramdownHeadingIds(out);
  out = convertLooseKramdownIds(out);
  return out;
}

function languageFromExt(file) {
  const ext = (file.split(".").pop() || "").toLowerCase();
  switch (ext) {
    case "lock":
      return "toml";
    case "sh":
      return "shell";
    case "mdx":
      return "markdown";
    case "tsx":
      return "ts";
    case "rs":
      return "rust";
    case "move":
      return "move";
    case "prisma":
      return "ts";
    default:
      return ext || "text";
  }
}

function isExcludedPath(absPath, excludePaths = []) {
  if (!excludePaths || excludePaths.length === 0) return false;
  const pAbs = path.resolve(absPath);
  return excludePaths.some((ex) => isInsideDir(pAbs, path.resolve(ex)));
}

function buildFetchPath(specPath, docsDir, baseAbsPath, excludePaths = []) {
  if (/^https?:\/\//i.test(specPath)) return specPath;
  if (typeof specPath === "string" && specPath.trim().startsWith("#")) {
    console.warn(`[remark-includes] Skipping anchor-only include: ${specPath}`);
    return null;
  }

  const parts = specPath.split("/");
  if (parts[0].startsWith("github:")) {
    const org = parts[0].slice("github:".length);
    const repo = parts[1];
    const rest = parts.slice(2).join("/");
    return `${GITHUB_RAW}/${org}/${repo}/${GITHUB_BRANCH}/${rest}`;
  }

  let absPath;
  if (specPath.startsWith("./") || specPath.startsWith("../")) {
    absPath = path.resolve(path.dirname(baseAbsPath), specPath);
  } else if (specPath.startsWith("/")) {
    absPath = path.resolve(docsDir, "." + specPath);
  } else {
    absPath = path.resolve(__dirname, "../../../..", specPath);
  }

  if (isExcludedPath(absPath, excludePaths)) {
    console.warn(`[remark-includes] Skipping excluded path: ${absPath}`);
    return null;
  }

  // Back-compat: skip legacy framework path in includes (prevents loops)
  if (absPath.includes(path.join("references", "framework"))) {
    console.warn(
      `[remark-includes] Skipping excluded framework path: ${absPath}`,
    );
    return null;
  }

  return absPath;
}

async function fetchHttps(url) {
  const maxRedirects = 5;
  return new Promise((resolve, reject) => {
    const go = (target, redirects = 0) => {
      https
        .get(target, (res) => {
          if (
            [301, 302, 303, 307, 308].includes(res.statusCode || 0) &&
            res.headers.location
          ) {
            if (redirects >= maxRedirects)
              return reject(new Error("Too many redirects: " + target));
            return go(res.headers.location, redirects + 1);
          }
          if (res.statusCode !== 200) {
            console.error(
              `[remark-includes] Failed to fetch ${target}: ${res.statusCode}`,
            );
            return resolve(`Error loading content`);
          }
          res.setEncoding("utf8");
          let data = "";
          res.on("data", (chunk) => (data += chunk));
          res.on("end", () => resolve(data));
        })
        .on("error", (err) => reject(err));
    };
    go(url);
  });
}

async function readSpec(fullPath) {
  if (!fullPath) return `<!-- Content excluded from processing -->`;
  if (/^https?:\/\//.test(fullPath)) return await fetchHttps(fullPath);
  if (!fs.existsSync(fullPath)) {
    console.error(`[remark-includes] Missing file: ${fullPath}`);
    return `Error loading content (missing file): ${fullPath}`;
  }
  return fs.readFileSync(fullPath, "utf8").replaceAll("\t", "  ");
}

function splitPathMarker(spec) {
  const hash = spec.indexOf("#");
  if (hash < 0) return { file: spec, marker: null };
  return { file: spec.slice(0, hash), marker: spec.slice(hash) };
}

function getMarkerName(mark, key) {
  return mark && mark.includes(key)
    ? mark.substring(mark.indexOf(key) + key.length).trim()
    : null;
}

function escapeRegex(s) {
  return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function captureBalanced(sub, open = '{', close = '}') {
  let depth = 0;
  for (let i = 0; i < sub.length; i++) {
    const ch = sub[i];
    if (ch === open) depth++;
    else if (ch === close) {
      depth--;
      if (depth === 0) return sub.slice(0, i + 1);
    }
  }
  return null;
}

// ---------- Docs-tag extraction ----------

function extractDocsTagBlock(fullText, markerWithHash) {
  const tag = markerWithHash.trim();
  // Capture the content between closing and opening docs tags.
  // Account for any )}; characters that might be added to the closing tag.
  // These characters are used to add closing syntax - useful when
  // you want to capture only first part of a code snippet.
  // Intentionally forcing the closing docs tag.
  const docTagRe = new RegExp(
    `\\/\\/\\s?docs::${escapeRegex(tag)}\\b[^\\n]*\\n([\\s\\S]*)\\/\\/\\s*docs::\\/\\s?${escapeRegex(tag)}\\b(?<closers>[)};]*)`,
    "m",
  );
  const matchTaggedContent = docTagRe.exec(fullText);
  if (!matchTaggedContent) {
    return {
      ok: false,
      content: `// Section '${tag}' not found or is not closed properly`,
    };
  }
  let taggedContent = matchTaggedContent[1];

  const pauseTagRe = new RegExp(
    `^[\\t ]*\\/\\/[\\t ]*docs::${escapeRegex(tag)}-pause[\\t ]*$[\\s\\S]*?^[\\t ]*\\/\\/[\\t ]*docs::${escapeRegex(tag)}-resume[\\t ]*\\n?`,
    "gm",
  );

  taggedContent = taggedContent.replace(pauseTagRe, "");

  const closers =
    (matchTaggedContent.groups && matchTaggedContent.groups.closers) ||
    matchTaggedContent[2] ||
    "";
  var closing = "";
  // Add the optional closing characters with proper spacing.
  if (/[)};]+/.test(closers)) {
    const closingTotal = closers.length;
    let closingArray = [];
    for (let i = 0; i < closingTotal; i++) {
      const currentChar = closers[i];
      const nextChar = closers[i + 1];

      if (nextChar === ";") {
        closingArray.push(currentChar + nextChar);
        i++;
      } else {
        closingArray.push(currentChar);
      }
    }
    const totClosings = closingArray.length;

    // Process any closing elements added in the closing comment of source code
    for (let j = 0; j < totClosings; j++) {
      let space = "  ".repeat(totClosings - 1 - j);
      closing += `\n${space}${closingArray[j]}`;
    }
  }
  taggedContent = utils.removeLeadingSpaces(taggedContent + closing);

  return { ok: true, content: taggedContent };
}

// ---------- {@inject} (code snippets remain as CodeBlock) ----------

async function processInject(
  spec,
  opts,
  docsDir,
  baseAbsPath,
  excludePaths = [],
) {
  const { file: specFile, marker } = splitPathMarker(spec);
  const language = languageFromExt(specFile);
  const isMove = language === "move";
  const isTs = language === "ts" || language === "js";
  const isRust = language === "rust";

  const hideTitle = /deepbook-ref|ts-sdk-ref/.test(specFile);
  let titleLabel = "",
    cleanedSpec = "",
    titleUrl = "";
  if (!hideTitle) {
    titleLabel = specFile.replace(/^(\.\/|\.\.\/)+/, "");
    cleanedSpec = specFile.replace(/^[.\/]+/, "");
    titleUrl = `https://github.com/MystenLabs/sui/blob/main/${cleanedSpec}`;
  }

  const fetchPath = buildFetchPath(
    specFile,
    docsDir,
    baseAbsPath,
    excludePaths,
  );

  let fileContent;
  if (/^https?:\/\//.test(fetchPath)) {
    fileContent = await fetchHttps(fetchPath);
  } else if (!fetchPath) {
    return `\n> Code injection skipped (excluded path): ${specFile}\n`;
  } else {
    if (!fs.existsSync(fetchPath)) {
      return `\n> Code to inject not found: ${specFile} --> ${fetchPath}\n`;
    }
    fileContent = fs.readFileSync(fetchPath, "utf8").replaceAll("\t", "  ");
  }

  if (marker) {
    const funKey = "#fun=";
    const structKey = "#struct=";
    const moduleKey = "#module=";
    const varKey = "#variable=";
    const useKey = "#use=";
    const componentKey = "#component=";
    const enumKey = "#enum=";
    const typeKey = "#type=";
    const traitKey = "#trait=";

    const funName = getMarkerName(marker, funKey);
    const structName = getMarkerName(marker, structKey);
    const moduleName = getMarkerName(marker, moduleKey);
    const variableName = getMarkerName(marker, varKey);
    const useName = getMarkerName(marker, useKey);
    const componentName = getMarkerName(marker, componentKey);
    const enumName = getMarkerName(marker, enumKey);
    const typeName = getMarkerName(marker, typeKey);
    const traitName = getMarkerName(marker, traitKey);

    if (funName) {
      const funs = funName.split(",");
      let funContent = [];
      for (let fn of funs) {
        fn = fn.trim();
        let funStr = "";
        if (isMove) {
          funStr = `^(\\s*)*?(pub(lic)? )?(entry )?fu?n \\b${escapeRegex(fn)}\\b[\\s\\S]*?^\\}`;
        } else if (isTs) {
          funStr = `^(\\s*)(async )?(export (default )?)?function \\b${escapeRegex(fn)}\\b[\\s\\S]*?\\n\\1\\}`;
        } else if (isRust) {
          funStr = `^(\\s*)(pub\\s+)?(async\\s+)?(const\\s+)?(unsafe\\s+)?(extern\\s+("[^"]+"\\s*)?)?fn\\s+${escapeRegex(fn)}\\s*(<[^>]*>)?\\s*\\([^)]*\\)\\s*(->\\s*[^;{]+)?\\s*[;{][\\s\\S]*?^\\}`;
        }
        const funRE = new RegExp(funStr, "ms");
        const funMatch = funRE.exec(fileContent);
        if (funMatch) {
          let pre = utils.capturePrepend(funMatch, fileContent);
          funContent.push(utils.removeLeadingSpaces(funMatch[0], pre));
        }
      }
      fileContent = funContent
        .join("\n")
        .replace(/\n{3,}/g, "\n\n")
        .trim();
    } else if (structName) {
      const structs = structName.split(",");
      let structContent = [];
      for (let s of structs) {
        s = s.trim();
        let structStr = `^(\\s*)\\b(pub(lic)?\\s+)?struct\\s+${escapeRegex(s)}\\b;\\s*$`;
        let structRE = new RegExp(structStr, "m");
        let structMatch = structRE.exec(fileContent);
        if (!structMatch) {
          structStr = `^(\\s*)*?(pub(lic)? )?struct \\b${escapeRegex(s)}\\b[\\s\\S]*?^\\}`;
          structRE = new RegExp(structStr, "ms");
          let m = structRE.exec(fileContent);
          if (m) structMatch = m;
        }
        if (structMatch) {
          const pre = utils.capturePrepend(structMatch, fileContent);
          structContent.push(utils.removeLeadingSpaces(structMatch[0], pre));
        } else {
          fileContent =
            "Struct not found. If code is formatted correctly, consider using code comments instead.";
        }
      }
      fileContent = structContent.join("\n").trim();
    } else if (traitName) {
      const traits = traitName.split(",");
      let traitContent = [];
      for (let t of traits) {
        t = t.trim();
        const traitStr = `^(\\s*)*?(pub(lic)? )?trait \\b${escapeRegex(t)}\\b[\\s\\S]*?^\\}`;
        const traitRE = new RegExp(traitStr, "ms");
        const traitMatch = traitRE.exec(fileContent);
        if (traitMatch) {
          let pre = utils.capturePrepend(traitMatch, fileContent);
          traitContent.push(utils.removeLeadingSpaces(traitMatch[0], pre));
        } else {
          fileContent =
            "Struct not found. If code is formatted correctly, consider using code comments instead.";
        }
      }
      fileContent = traitContent.join("\n").trim();
    } else if (variableName) {
      const names = variableName.split(",");
      let out = [];
      if (isTs) {
        const varTsFunction = `^( *)?.*?(let|const) \\b${escapeRegex(variableName)}\\b.*=>`;
        const varTsVariable = `^( *)?.*?(let|const) \\b${escapeRegex(variableName)}\\b (?!.*=>)=.*;`;
        const reFun = new RegExp(varTsFunction, "m");
        const reVar = new RegExp(varTsVariable, "m");
        const mFun = reFun.exec(fileContent);
        const mVar = reVar.exec(fileContent);
        if (mFun) {
          const start = fileContent.slice(mFun.index);
          const endText = `^${mFun[1] ? mFun[1] : ""}\\)?\\};`;
          const endRE = new RegExp(endText, "m");
          const endMatch = endRE.exec(start);
          let pre = utils.capturePrepend(mFun, fileContent);
          out.push(
            utils.removeLeadingSpaces(
              start.slice(0, endMatch.index + endMatch[0].length),
              pre,
            ),
          );
        } else if (mVar) {
          let pre = utils.capturePrepend(mVar, fileContent);
          out.push(utils.removeLeadingSpaces(mVar[0], pre));
        } else {
          fileContent =
            "Variable not found. If code is formatted correctly, consider using code comments instead.";
        }
      } else {
        for (let v of names) {
          v = v.trim();
          const shortRe = new RegExp(
            `^(\\s*)?(#\\[test_only\\])?(let|const) \\(?.*?\\b${escapeRegex(v)}\\b.*?\\)?\\s?=.*;`,
            "m",
          );
          const longRe = new RegExp(
            `^(\\s*)?(#\\[test_only\\])?(let|const) \\(?.*?\\b${escapeRegex(v)}\\b.*?\\)?\\s?= \\{[^}]*\\};\\s*$`,
            "m",
          );
          const mShort = shortRe.exec(fileContent);
          const mLong = longRe.exec(fileContent);
          const m = mShort || mLong;
          if (m) {
            let pre = utils.capturePrepend(m, fileContent);
            out.push(utils.removeLeadingSpaces(m[0], pre));
          } else {
            fileContent =
              "Variable not found. If code is formatted correctly, consider using code comments instead.";
          }
        }
      }
      fileContent = out.join("\n").trim();
    } else if (getMarkerName(marker, "#use=")) {
      const uses = getMarkerName(marker, "#use=").split(",");
      let out = [];
      for (let u of uses) {
        const [base, last] = u.trim().split("::");
        const useStr = `^( *)(#\\[test_only\\] )?use ${escapeRegex(base)}::\\{?.*?${last ? escapeRegex(last) : ""}.*?\\};`;
        const re = new RegExp(useStr, "ms");
        const m = re.exec(fileContent);
        if (m) {
          let pre = utils.capturePrepend(m, fileContent);
          out.push(utils.removeLeadingSpaces(m[0], pre));
        } else {
          fileContent =
            "Use statement not found. If code is formatted correctly, consider using code comments instead.";
        }
      }
      fileContent = out.join("\n").trim();
    } else if (getMarkerName(marker, "#component=")) {
      const components = getMarkerName(marker, "#component=").split(",");
      let out = [];
      for (let comp of components) {
        let name = comp,
          element = "",
          ordinal = "";
        if (comp.includes(":")) {
          const parts = comp.split(":");
          name = parts[0];
          element = parts[1];
          ordinal = parts[2] || "";
        }
        const compStr = `^( *)(export (default )?)?function \\b${escapeRegex(name)}\\b[\\s\\S]*?\\n\\1\\}`;
        const re = new RegExp(compStr, "ms");
        const m = re.exec(fileContent);
        if (m) {
          if (element) {
            const elRe = new RegExp(
              `^( *)\\<${escapeRegex(element)}\\b[\\s\\S]*?\\<\\/${escapeRegex(element)}\\>`,
              "msg",
            );
            let keep = [1];
            if (ordinal.includes("-") && !ordinal.includes("&")) {
              const [a, b] = ordinal.split("-").map(Number); // (typo prevention)
            }
          } else {
            let pre = utils.capturePrepend(m, fileContent);
            out.push(utils.removeLeadingSpaces(m[0], pre));
          }
        }
      }
      fileContent = out.join("\n").trim();
    } else if (getMarkerName(marker, "#module=")) {
      const moduleName = getMarkerName(marker, "#module=");
      const modStr = `^(\\s*)*module \\b${escapeRegex(moduleName)}\\b[\\s\\S]*?^\\}`;
      const re = new RegExp(modStr, "ms");
      const m = re.exec(fileContent);
      if (m) {
        const pre = utils.capturePrepend(m, fileContent);
        fileContent = utils.removeLeadingSpaces(m[0], pre);
      } else {
        fileContent =
          "Module not found. If code is formatted correctly, consider using code comments instead.";
      }
    } else if (getMarkerName(marker, "#enum=")) {
      const enums = getMarkerName(marker, "#enum=").split(",").map(s => s.trim()).filter(Boolean);
      const out = [];
      for (const e of enums) {
        // Match optional keywords: export / declare / const (TS) OR pub (Rust)
        const re = new RegExp(
          `^( *)(?:export\\s+)?(?:declare\\s+)?(?:const\\s+)?(?:pub\\s+)?enum\\s+${escapeRegex(e)}\\s*\\{`,
          "m",
        );
        const m = re.exec(fileContent);
        if (m) {
          const start = m.index;
          const sub = fileContent.slice(start);
          const openIdx = sub.indexOf("{");
          if (openIdx !== -1) {
            const block = captureBalanced(sub.slice(openIdx));
            if (block) {
              out.push(
                utils.removeLeadingSpaces(sub.slice(0, openIdx) + block)
              );
            }
          }
        }
      }
      fileContent = out.join("\n").trim();
    } else if (getMarkerName(marker, "#type=")) {
      const types = getMarkerName(marker, "#type=").split(",");
      let out = [];
      for (let t of types) {
        const startRe = new RegExp(
          `^( *)(export )?type \\b${escapeRegex(t)}\\b`,
          "m",
        );
        const m = startRe.exec(fileContent);
        if (m) {
          let sub = fileContent.slice(m.index);
          const spaces = m[1] || "";
          const endRe = new RegExp(`^${spaces}\\};`, "m");
          const e = endRe.exec(sub);
          if (e)
            out.push(
              utils.removeLeadingSpaces(sub.slice(0, e.index + e[0].length)),
            );
          else out.push("Error capturing type declaration.");
        }
      }
      fileContent = out.join("\n").trim();
    } else if (getMarkerName(marker, "#trait=")) {
      const traits = getMarkerName(marker, "#trait=").split(",");
      let out = [];
      for (let t of traits) {
        const traitStr = `^(\\s*)*?(pub(lic)? )?trait \\b${escapeRegex(t)}\\b[\\s\\S]*?^\\}`;
        const re = new RegExp(traitStr, "ms");
        const m = re.exec(fileContent);
        if (m) {
          let pre = utils.capturePrepend(m, fileContent);
          out.push(utils.removeLeadingSpaces(m[0], pre));
        } else {
          fileContent =
            "Struct not found. If code is formatted correctly, consider using code comments instead.";
        }
      }
      fileContent = out.join("\n").trim();
    } else {
      const { ok, content } = extractDocsTagBlock(fileContent, marker);
      if (!ok) return content;
      fileContent = content;
    }
  }

  const processed = utils.processOptions(fileContent, opts);
  const titleProp = hideTitle
    ? ""
    : ` title={<a href="${titleUrl}" target="_blank" rel="noopener noreferrer">${titleLabel}</a>}`;
  return (
    `<CodeBlock language="${language}"${titleProp}>{${JSON.stringify(processed)}}` +
    `</CodeBlock>`
  );
}

// ---------- Nested directive expansion (only for {@include} text) ----------

const RE_INCLUDE_TXT = /^\s*\{@include:\s*([^\s}]+)\s*\}\s*$/;
const RE_INCLUDE_HTML = /^\s*<!--\s*\{@include:\s*([^\s}]+)\s*\}\s*-->\s*$/;
const RE_INJECT_TXT = /^\s*\{@inject:\s*([^\s}]+)(?:\s+([^}]*?))?\s*\}\s*$/;
const RE_INJECT_HTML =
  /^\s*<!--\s*\{@inject:\s*([^\s}]+)(?:\s+([^}]*?))?\s*\}\s*-->\s*$/;

const RE_INCLUDE_INLINE = /{\s*@include:\s*([^\s}]+)\s*}/g;
const RE_INCLUDE_INLINE_HTML = /<!--\s*{\s*@include:\s*([^\s}]+)\s*}\s*-->/g;

function stripImportStatements(text) {
  return text.replace(/^[ \t]*import\s+.*$/gm, "").replace(/\n{3,}/g, "\n\n");
}

async function expandDirectivesInText(
  markdown,
  docsDir,
  baseAbsPath,
  excludePaths = [],
  depth = 0,
  seenStack = [],
) {
  const MAX_PASSES = 4; // keep small to avoid runaway recursion
  const MAX_DEPTH = 10;
  if (depth > MAX_DEPTH) return markdown;

  let text = markdown;

  async function replaceAllAsyncInline(s, re, replacer) {
    let out = "";
    let lastIdx = 0;
    for (const m of s.matchAll(re)) {
      out += s.slice(lastIdx, m.index);
      out += await replacer(m);
      lastIdx = m.index + m[0].length;
    }
    out += s.slice(lastIdx);
    return out;
  }

  for (let pass = 0; pass < MAX_PASSES; pass++) {
    let changed = false;

    const expandIncludeSpec = async (specRaw) => {
      const spec = specRaw.trim();

      if (!spec || spec[0] === "#") {
        return `<!-- Include skipped (anchor-only): ${specRaw} -->`;
      }

      const fullPath = buildFetchPath(spec, docsDir, baseAbsPath, excludePaths);
      let newBase = baseAbsPath;
      let included = "";
      if (/^https?:\/\//.test(fullPath)) {
        included = await readSpec(fullPath);
      } else if (!fullPath) {
        return `<!-- Include skipped (excluded path): ${spec} -->`;
      } else {
        included = await readSpec(fullPath);
        newBase = fullPath;
      }

      const key = `${depth}:${fullPath}`;
      if (seenStack.includes(key)) return included;
      const nextSeen = seenStack.concat(key);

      const expanded = await expandDirectivesInText(
        included,
        docsDir,
        newBase,
        excludePaths,
        depth + 1,
        nextSeen,
      );
      return stripImportStatements(expanded);
    };

    const beforeInclude = text;
    text = await replaceAllAsyncInline(text, RE_INCLUDE_INLINE, async (m) =>
      expandIncludeSpec((m[1] || "").trim()),
    );
    text = await replaceAllAsyncInline(
      text,
      RE_INCLUDE_INLINE_HTML,
      async (m) => expandIncludeSpec((m[1] || "").trim()),
    );
    if (text !== beforeInclude) changed = true;
    if (!changed) break;
  }
  return text;
}

// ---------- The remark plugin ----------

module.exports = function remarkIncludes(options) {
  const docsDir = options?.docsDir || process.cwd();

  const heavyDirs = heavyHtmlDirs(docsDir);
  const userExcludes = options?.excludePaths || [];
  const excludeDirsAbs = [...userExcludes.map((p) => path.resolve(p))];

  return async (tree, file) => {
    const filePath = file.history?.[0] || file.path || "";
    const normalizedFilePath = path.resolve(filePath);

    // Heavy Markdown doc: normalize headings/anchors, convert to HTML, then inject via JSX
    for (const dirAbs of heavyDirs) {
      if (isInsideDir(normalizedFilePath, dirAbs)) {
        let raw = String(file.value ?? "");
        try {
          raw = fs.readFileSync(normalizedFilePath, "utf8");
        } catch (_) {}

        const normalized = normalizeFrameworkMarkdown(raw);
        let html = markdownToHtml(normalized);
        html = fixAngleBracketAutolinksInHtml(html);
        const safeHtml = fixAnchorOnlyUrlsInHtml(html);

        tree.children = [jsxDivWithInlineHtml(safeHtml)];

        // Optional sentinel
        file.data = file.data || {};
        file.data.__renderedAsLiteral = true;
        return;
      }
    }

    // Normal path: look for include/inject directives
    let needsCodeBlockImport = false;
    const replacements = [];

    visitParents(tree, (node, ancestors) => {
      const parent = ancestors[ancestors.length - 1];
      if (!parent || (node.type !== "text" && node.type !== "html")) return;

      const value = node.value || "";
      let m;

      if (
        (m = value.match(RE_INCLUDE_TXT)) ||
        (m = value.match(RE_INCLUDE_HTML))
      ) {
        const spec = (m[1] || "").trim();
        const { container, index } = pickBlockContainer(node, ancestors);
        if (index >= 0) {
          // 1) If it's anchor-only (#{...} or just #), drop the directive entirely.
          if (!spec || spec[0] === "#") {
            container.splice(index, 1);
          } else {
            replacements.push({ container, index, kind: "include", spec });
          }
        }
        return;
      }

      if (
        (m = value.match(RE_INJECT_TXT)) ||
        (m = value.match(RE_INJECT_HTML))
      ) {
        const spec = m[1].trim();
        const rest = (m[2] || "").trim();
        const { container, index } = pickBlockContainer(node, ancestors);
        if (index >= 0)
          replacements.push({ container, index, kind: "inject", spec, rest });
        return;
      }
    });

    // Do replacements in reverse to keep indices valid
    for (const r of replacements.reverse()) {
      const { container, index, kind, spec } = r;

      if (kind === "include") {
        // Read include, expand ONLY nested {@include}, then decide rendering path.
        const fullPath = buildFetchPath(
          spec,
          docsDir,
          file.history?.[0] || file.path,
          excludeDirsAbs,
        );
        const raw = await readSpec(fullPath);
        const baseAbsPath = /^https?:\/\//.test(fullPath)
          ? file.history?.[0] || file.path
          : fullPath;
        const expanded = await expandDirectivesInText(
          raw,
          docsDir,
          baseAbsPath,
          excludeDirsAbs,
        );

        // Heavy include: normalize -> HTML -> inject via JSX
        if (fullPath && !/^https?:\/\//.test(fullPath)) {
          const abs = path.resolve(fullPath);
          if (heavyDirs.some((d) => isInsideDir(abs, d))) {
            const expandedText = expanded;
            // If include has an admonition, let Docusaurus' admonition plugin transform it.
            // i.e., don't bypass to HTML — parse to MD AST so downstream plugins can run.
            if (containsAdmonition(expandedText)) {
              const nodes = parseMarkdownToNodes(
                stripImportStatements(expandedText),
              );
              container.splice(index, 1, ...nodes);
              continue;
            }

            // Otherwise keep the fast heavy path (MD → HTML → inject)
            let html = markdownToHtml(normalizeFrameworkMarkdown(expandedText));
            html = fixAngleBracketAutolinksInHtml(html);
            const safeHtml = fixAnchorOnlyUrlsInHtml(html);
            container.splice(index, 1, jsxDivWithInlineHtml(safeHtml));
            continue;
          }
        }

        // Otherwise parse normally
        const nodes = parseMarkdownToNodes(stripImportStatements(expanded));
        container.splice(index, 1, ...nodes);
      } else {
        // {@inject}: produce JSX CodeBlock (title suppressed if deepbook-ref)
        const opts = r.rest ? r.rest.split(/\s+/).filter(Boolean) : [];
        const jsx = await processInject(
          spec,
          opts,
          docsDir,
          file.history?.[0] || file.path,
          excludeDirsAbs,
        );
        const nodes = parseMarkdownToNodes(jsx);
        container.splice(index, 1, ...nodes);
        needsCodeBlockImport = true;
      }
    }

    // Import CodeBlock if we emitted it via {@inject}
    if (needsCodeBlockImport) {
      tree.children.unshift({
        type: "mdxjsEsm",
        value: `import CodeBlock from '@theme/CodeBlock';`,
      });
    }
  };
};

function fixAnchorOnlyUrlsInHtml(html) {
  // replace href="#" / src="#" / poster="#"
  return html
    .replace(/\bhref="#"/g, 'href="#_"')
    .replace(/\bsrc="#"/g, 'src="#_"')
    .replace(/\bposter="#"/g, 'poster="#_"');
}

// ---------- Block container picker (kept at bottom for clarity) ----------

function pickBlockContainer(node, ancestors) {
  const parent = ancestors[ancestors.length - 1];
  const grand = ancestors[ancestors.length - 2];

  let container = parent?.children || [];
  let index = container.indexOf(node);

  if (
    parent &&
    parent.type === "paragraph" &&
    Array.isArray(parent.children) &&
    parent.children.length === 1 &&
    grand &&
    Array.isArray(grand.children)
  ) {
    container = grand.children;
    index = container.indexOf(parent);
  }
  return { container, index };
}
