// remark-includes.cjs
// Replaces the @docusaurus-plugin-includes feature (Docusaurus 3.x compatible)
// Adds {@include: path} and {@inject: path[#selector] [options...]} handling
//
// - {@include: ...} injects raw markdown (parsed with MDX+GFM+Directives).
// - {@inject: ...} injects formatted fenced code blocks using the SAME logic as injectLoader.js,
//   plus robust docs-tag extraction using lines like:
//       // docs::#tag
//       ...snippet...
//       // docs::/#tag
//
// Title link behavior:
//   The code fence title is a clickable link that ALWAYS points to:
//     https://github.com/MystenLabs/sui/blob/main/<path>
//   The displayed label strips any leading "./" or "../".
//
// Nested directives:
//   Only {@include} appearing *inside* included markdown is expanded **before** MDX parsing.
//   {@inject} inside included content is intentionally NOT supported.
//
// SPDX-License-Identifier: Apache-2.0

const fs = require("fs");
const path = require("path");
const https = require("https");
const utils = require("./utils.js");

const { visitParents } = require("unist-util-visit-parents");
const { fromMarkdown } = require("mdast-util-from-markdown");
const { mdxFromMarkdown } = require("mdast-util-mdx");
const { mdxjs } = require("micromark-extension-mdxjs");
const { gfm } = require("micromark-extension-gfm");
const { gfmFromMarkdown } = require("mdast-util-gfm");
const { directive } = require("micromark-extension-directive");
const { directiveFromMarkdown } = require("mdast-util-directive");

// ---------- Shared helpers ----------

const GITHUB_RAW = "https://raw.githubusercontent.com";
const GITHUB_BRANCH = "main";

// Redirect-aware HTTPS fetch that concatenates chunks
function fetchHttps(url) {
  const maxRedirects = 5;
  return new Promise((resolve, reject) => {
    const go = (target, redirects = 0) => {
      https
        .get(target, (res) => {
          if (
            [301, 302, 303, 307, 308].includes(res.statusCode || 0) &&
            res.headers.location
          ) {
            if (redirects >= maxRedirects) {
              return reject(new Error("Too many redirects: " + target));
            }
            return go(res.headers.location, redirects + 1);
          }
          if (res.statusCode !== 200) {
            console.error(`[remark-includes] Failed to fetch ${target}: ${res.statusCode}`);
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

function parseMarkdownToNodes(markdownText) {
  const tree = fromMarkdown(markdownText, {
    extensions: [mdxjs(), gfm(), directive()],
    mdastExtensions: [mdxFromMarkdown(), gfmFromMarkdown(), directiveFromMarkdown()],
  });
  return tree.children || [];
}

// Choose container/index to replace a whole block when directive is the only child of a paragraph
function pickBlockContainer(node, ancestors) {
  const parent = ancestors[ancestors.length - 1];
  const grand = ancestors[ancestors.length - 2];

  // Default: replace the node itself inside its parent
  let container = parent?.children || [];
  let index = container.indexOf(node);

  // If the directive is the only child of a paragraph, replace the entire paragraph
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

// ---------- Path / language helpers (aligned with injectLoader.js) ----------

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

// Helper function to check if a path should be excluded
function isExcludedPath(absPath, excludePaths = []) {
  if (!excludePaths || excludePaths.length === 0) return false;
  
  const normalizedPath = path.normalize(absPath);
  
  return excludePaths.some(excludePath => {
    const normalizedExclude = path.resolve(excludePath);
    return normalizedPath.includes(normalizedExclude) || 
           normalizedPath.includes(path.normalize(excludePath));
  });
}

function buildFetchPath(specPath, docsDir, baseAbsPath, excludePaths = []) {
  if (/^https?:\/\//i.test(specPath)) return specPath;

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
    absPath = path.resolve(docsDir, specPath);
  }

  // Check exclusion paths
  if (isExcludedPath(absPath, excludePaths)) {
    console.warn(`[remark-includes] Skipping excluded path: ${absPath}`);
    return null;
  }

  // Legacy hardcoded exclusion (keeping for backwards compatibility)
  if (absPath.includes(path.join("references", "framework"))) {
    console.warn(`[remark-includes] Skipping excluded framework path: ${absPath}`);
    return null;
  }

  return absPath;
}

// Keep this around (used elsewhere previously); not harmful if unused.
function buildTitleLink(specPath) {
  const stripDotSegments = (p) => p.replace(/^(\.\/|\.\.\/)+/, "");
  const label = stripDotSegments(specPath);
  const cleanedSpec = specPath.replace(/^[.\/]+/, "");
  const fullUrl = `https://github.com/MystenLabs/sui/blob/main/${cleanedSpec}`;
  return `[${label}](${fullUrl})`;
}

async function readSpec(fullPath) {
  if (!fullPath) {
    return `<!-- Content excluded from processing -->`;
  }

  if (/^https?:\/\//.test(fullPath)) {
    return await fetchHttps(fullPath);
  }
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
  return mark && mark.includes(key) ? mark.substring(mark.indexOf(key) + key.length).trim() : null;
}

function escapeRegex(s) {
  return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

// ---------- Robust docs-tag extraction ----------
//
// Matches blocks delimited by lines like:
//   // docs::#tag
//   ...content...
//   // docs::/#tag
//
// Also supports pause/resume inside the block:
//   // docs::#tag-pause[:replacement]
//   ...skipped...
//   // docs::#tag-resume
//

function extractDocsTagBlock(fullText, markerWithHash) {
  const tag = markerWithHash.trim(); // e.g. "#bound"

  // allow trailing comment after the tag on the start line
  const startRe = new RegExp(
    `^\\s*//\\s*docs::\\s*${escapeRegex(tag)}(?:\\s+.*)?$`,
    "m"
  );

  // allow ):; closers and optional trailing comment after end tag
  const endReWithClosers = new RegExp(
    `^\\s*//\\s*docs::/\\s*${escapeRegex(tag)}\\s*([)};:]*)\\s*(?:.*)?$`,
    "m"
  );

  const startMatch = startRe.exec(fullText);
  if (!startMatch) {
    return { ok: false, content: `// Section '${tag}' not found` };
  }
  const afterStartIdx = startMatch.index + startMatch[0].length;
  const tail = fullText.slice(afterStartIdx);

  const endMatch = endReWithClosers.exec(tail);
  if (!endMatch) {
    return { ok: false, content: `// Section '${tag}' end not found` };
  }

  const block = tail.slice(0, endMatch.index);
  const closers = endMatch[1] || "";
  return { ok: true, content: block + closers };
}

// ---------- {@inject: ...} processor (ported & aligned with injectLoader.js) ----------

async function processInject(spec, opts, docsDir, baseAbsPath, excludePaths = []) {
  // spec: "path[#selector]"
  const { file: specFile, marker } = splitPathMarker(spec);
  const language = languageFromExt(specFile);
  const isMove = language === "move";
  const isTs = language === "ts" || language === "js";
  const isRust = language === "rust";

  // Build clickable title parts
  const titleLabel = specFile.replace(/^(\.\/|\.\.\/)+/, "");
  const cleanedSpec = specFile.replace(/^[.\/]+/, "");
  const titleUrl = `https://github.com/MystenLabs/sui/blob/main/${cleanedSpec}`;

  const fetchPath = buildFetchPath(specFile, docsDir, baseAbsPath, excludePaths);

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

  // If a marker/selector was provided, replicate loader logic
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
      fileContent = funContent.join("\n").replace(/\n{3,}/g, "\n\n").trim();
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
          structMatch = structRE.exec(fileContent);
        }
        if (structMatch) {
          const pre = utils.capturePrepend(structMatch, fileContent);
          structContent.push(utils.removeLeadingSpaces(structMatch[0], pre));
        } else {
          fileContent = "Struct not found. If code is formatted correctly, consider using code comments instead.";
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
          fileContent = "Struct not found. If code is formatted correctly, consider using code comments instead.";
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
          out.push(utils.removeLeadingSpaces(start.slice(0, endMatch.index + endMatch[0].length), pre));
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
          const shortRe = new RegExp(`^(\\s*)?(#\\[test_only\\])?(let|const) \\(?.*?\\b${escapeRegex(v)}\\b.*?\\)?\\s?=.*;`, "m");
          const longRe = new RegExp(`^(\\s*)?(#\\[test_only\\])?(let|const) \\(?.*?\\b${escapeRegex(v)}\\b.*?\\)?\\s?= \\{[^}]*\\};\\s*$`, "m");
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
    } else if (useName) {
      const uses = useName.split(",");
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
    } else if (componentName) {
      const components = componentName.split(",");
      let out = [];
      for (let comp of components) {
        let name = comp, element = "", ordinal = "";
        if (comp.includes(":")) {
          const parts = comp.split(":");
          name = parts[0]; element = parts[1]; ordinal = parts[2] || "";
        }
        const compStr = `^( *)(export (default )?)?function \\b${escapeRegex(name)}\\b[\\s\\S]*?\\n\\1\\}`;
        const re = new RegExp(compStr, "ms");
        const m = re.exec(fileContent);
        if (m) {
          if (element) {
            const elRe = new RegExp(`^( *)\\<${escapeRegex(element)}\\b[\\s\\S]*?\\<\\/${escapeRegex(element)}\\>`, "msg");
            let keep = [1];
            if (ordinal.includes("-") && !ordinal.includes("&")) {
              const [a, b] = ordinal.split("-").map(Number);
              keep = Array.from({ length: b - a + 1 }, (_, i) => a + i);
            } else if (ordinal.includes("&")) {
              keep = ordinal.split("&").map(Number);
            }
            keep.sort((a, b) => a - b);
            for (let x = 0; x < keep[keep.length - 1]; x++) {
              const elMatch = elRe.exec(m[0]);
              if (keep.includes(x + 1) && elMatch) {
                out.push(utils.removeLeadingSpaces(elMatch[0]));
              } else if (x > 0 && out[out.length - 1]?.trim() !== "...") {
                out.push("\n...");
              }
            }
          } else {
            let pre = utils.capturePrepend(m, fileContent);
            out.push(utils.removeLeadingSpaces(m[0], pre));
          }
        }
      }
      fileContent = out.join("\n").trim();
    } else if (moduleName) {
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
    } else if (enumName) {
      const enums = enumName.split(",");
      let out = [];
      for (let e of enums) {
        const re = new RegExp(`^( *)(export)? enum \\b${escapeRegex(e)}\\b\\s*\\{[\\s\\S]*?\\}`, "m");
        const m = re.exec(fileContent);
        if (m) out.push(utils.removeLeadingSpaces(m[0]));
      }
      fileContent = out.join("\n").trim();
    } else if (typeName) {
      const types = typeName.split(",");
      let out = [];
      for (let t of types) {
        const startRe = new RegExp(`^( *)(export )?type \\b${escapeRegex(t)}\\b`, "m");
        const m = startRe.exec(fileContent);
        if (m) {
          let sub = fileContent.slice(m.index);
          const spaces = m[1] || "";
          const endRe = new RegExp(`^${spaces}\\};`, "m");
          const e = endRe.exec(sub);
          if (e) {
            sub = sub.slice(0, e.index + e[0].length);
            out.push(utils.removeLeadingSpaces(sub));
          } else {
            out.push("Error capturing type declaration.");
          }
        }
      }
      fileContent = out.join("\n").trim();
    } else {
      // Fallback: docs-tag block extraction (#tag)
      const { ok, content } = extractDocsTagBlock(fileContent, marker);
      if (!ok) return content;
      fileContent = content;
    }
  }

  // Apply post options & formatting using utils (same as loader)
  const processed = utils.processOptions(fileContent, opts);

  // Emit JSX CodeBlock so the title can be a real <a> link.
  // Use JSON.stringify to avoid backticks inside MDX/Acorn parsing.
  const jsx =
    `<CodeBlock language="${language}" ` +
    `title={<a href="${titleUrl}" target="_blank" rel="noopener noreferrer">${titleLabel}</a>}>` +
    `{${JSON.stringify(processed)}}` +
    `</CodeBlock>`;
  return jsx;
}

// ---------- Nested directive expansion inside included markdown ----------

// Strict (full-line) forms used when scanning the Markdown AST
const RE_INCLUDE_TXT = /^\s*\{@include:\s*([^\s}]+)\s*\}\s*$/;
const RE_INCLUDE_HTML = /^\s*<!--\s*\{@include:\s*([^\s}]+)\s*\}\s*-->\s*$/;
const RE_INJECT_TXT = /^\s*\{@inject:\s*([^\s}]+)(?:\s+([^}]*?))?\s*\}\s*$/;
const RE_INJECT_HTML = /^\s*<!--\s*\{@inject:\s*([^\s}]+)(?:\s+([^}]*?))?\s*\}\s*-->\s*$/;

// Lenient, inline-capable patterns for raw text replacement inside includes (ONLY include)
const RE_INCLUDE_INLINE = /{\s*@include:\s*([^\s}]+)\s*}/g;
const RE_INCLUDE_INLINE_HTML = /<!--\s*{\s*@include:\s*([^\s}]+)\s*}\s*-->/g;

// Remove MDX import lines from included snippets
function stripImportStatements(text) {
  // Drop lines that start with "import ..." (typical MDX/ESM import lines)
  return text.replace(/^[ \t]*import\s+.*$/gm, "").replace(/\n{3,}/g, "\n\n");
}

// Recursively expand {@include:...} found in raw text, BEFORE MDX parsing.
// - baseAbsPath tracks the current file for resolving relative paths in nested includes.
async function expandDirectivesInText(markdown, docsDir, baseAbsPath, excludePaths = [], depth = 0, seenStack = []) {
  const MAX_PASSES = 6;
  const MAX_DEPTH = 20;
  if (depth > MAX_DEPTH) return markdown;

  let text = markdown;

  // Helper: async global replace for inline patterns
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

    // Expand all {@include:...} (inline/comment) by recursively loading and expanding
    const expandIncludeSpec = async (specRaw) => {
      const spec = specRaw.trim();
      const fullPath = buildFetchPath(spec, docsDir, baseAbsPath, excludePaths);
      // derive a new base path for nested relative resolution
      let newBase = baseAbsPath;
      let included = "";
      if (/^https?:\/\//.test(fullPath)) {
        included = await readSpec(fullPath);
        // keep newBase as current baseAbsPath for URL includes
      } else if (!fullPath) {
        return `<!-- Include skipped (excluded path): ${spec} -->`;
      } else {
        included = await readSpec(fullPath);
        newBase = fullPath; // now relative includes resolve to this file's dir
      }

      // Prevent trivial cycles
      const key = `${depth}:${fullPath}`;
      if (seenStack.includes(key)) return included;
      const nextSeen = seenStack.concat(key);

      // Recursively expand ONLY {@include} inside the included content
      const expanded = await expandDirectivesInText(included, docsDir, newBase, excludePaths, depth + 1, nextSeen);
      // Strip MDX import lines from the expanded include
      return stripImportStatements(expanded);
    };

    const beforeInclude = text;
    text = await replaceAllAsyncInline(text, RE_INCLUDE_INLINE, async (m) => {
      const spec = (m[1] || "").trim();
      return await expandIncludeSpec(spec);
    });
    text = await replaceAllAsyncInline(text, RE_INCLUDE_INLINE_HTML, async (m) => {
      const spec = (m[1] || "").trim();
      return await expandIncludeSpec(spec);
    });
    if (text !== beforeInclude) changed = true;

    if (!changed) break;
  }

  return text;
}

// ---------- The remark plugin ----------

module.exports = function remarkIncludes(options) {
  const docsDir = options?.docsDir || process.cwd();
  const excludePaths = options?.excludePaths || [];

  return async (tree, file) => {
    // Skip processing for files in excluded paths
    const filePath = file.history?.[0] || file.path || "";
    const normalizedFilePath = path.normalize(filePath);
    
    // Check if current file should be excluded
    const shouldSkip = excludePaths.some(excludePath => {
      const normalizedExclude = path.resolve(excludePath);
      return normalizedFilePath.includes(normalizedExclude) || 
             normalizedFilePath.includes(path.normalize(excludePath));
    }) || normalizedFilePath.includes(path.join("references", "framework"));

    if (shouldSkip) {
      console.log(`[remark-includes] Skipping excluded file: ${filePath}`);
      return; // Exit early, don't process this file at all
    }

    let needsCodeBlockImport = false;
    const replacements = [];

    visitParents(tree, (node, ancestors) => {
      const parent = ancestors[ancestors.length - 1];
      if (!parent || (node.type !== "text" && node.type !== "html")) return;

      const value = node.value || "";
      let m;

      if ((m = value.match(RE_INCLUDE_TXT)) || (m = value.match(RE_INCLUDE_HTML))) {
        const spec = m[1].trim();
        const { container, index } = pickBlockContainer(node, ancestors);
        if (index >= 0) {
          replacements.push({ container, index, kind: "include", spec });
        }
        return;
      }

      if ((m = value.match(RE_INJECT_TXT)) || (m = value.match(RE_INJECT_HTML))) {
        const spec = m[1].trim();
        const rest = (m[2] || "").trim();
        const { container, index } = pickBlockContainer(node, ancestors);
        if (index >= 0) {
          replacements.push({ container, index, kind: "inject", spec, rest });
        }
        return;
      }
    });

    // Perform async replacements in reverse order to keep indices valid
    for (const r of replacements.reverse()) {
      const { container, index, kind, spec } = r;

      if (kind === "include") {
        // Read include, expand ONLY nested {@include}, strip MDX imports, then parse.
        const fullPath = buildFetchPath(spec, docsDir, file.history?.[0] || file.path, excludePaths);
        let raw = await readSpec(fullPath);

        // baseAbsPath for resolving relatives within this include
        const baseAbsPath = /^https?:\/\//.test(fullPath) ? (file.history?.[0] || file.path) : fullPath;

        // Expand ONLY nested {@include} and strip MDX import lines
        const expanded = stripImportStatements(
          await expandDirectivesInText(raw, docsDir, baseAbsPath, excludePaths)
        );

        const nodes = parseMarkdownToNodes(expanded);
        container.splice(index, 1, ...nodes);
      } else {
        // inject: produce JSX CodeBlock (clickable title)
        const opts = r.rest ? r.rest.split(/\s+/).filter(Boolean) : [];
        const jsxBlock = await processInject(
          spec,
          opts,
          docsDir,
          file.history?.[0] || file.path,
          excludePaths
        );
        const nodes = parseMarkdownToNodes(jsxBlock);
        container.splice(index, 1, ...nodes);
        needsCodeBlockImport = true; // we emitted <CodeBlock/>
      }
    }

    // Prepend the import exactly once if we emitted any JSX CodeBlock.
    if (needsCodeBlockImport) {
      tree.children.unshift({
        type: "mdxjsEsm",
        value: `import CodeBlock from '@theme/CodeBlock';`,
      });
    }
  };
};