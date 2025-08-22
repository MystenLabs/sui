// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React from 'react';
import CodeBlock from '@theme/CodeBlock';

const GITHUB_REPO = 'MystenLabs/sui';
const GITHUB_BRANCH = 'main';

function inferLanguage(filePath, fallback = 'text') {
  if (!filePath) return fallback;
  const pure = filePath.split('#')[0] || filePath;
  const ext = (pure.split('.').pop() || '').toLowerCase();
  switch (ext) {
    case 'rs':
      return 'rust';
    case 'ts':
    case 'tsx':
      return 'ts';
    case 'js':
    case 'jsx':
      return 'js';
    case 'json':
      return 'json';
    case 'md':
    case 'mdx':
      return 'markdown';
    case 'sh':
      return 'shell';
    case 'lock':
      return 'toml';
    case 'move':
      return 'move';
    default:
      return ext || fallback;
  }
}

function escapeRegex(s) {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

function removeLeadingSpaces(text) {
  const lines = text.replace(/\t/g, '  ').split('\n');
  let min = Infinity;
  for (const ln of lines) {
    if (!ln.trim()) continue;
    const m = ln.match(/^ */);
    if (m) min = Math.min(min, m[0].length);
  }
  if (!isFinite(min)) return text.trimEnd();
  return lines.map((ln) => ln.slice(min)).join('\n').trimEnd();
}

// ---------- docs-tag block extraction ----------
// // docs::#tag
//   ...content...
// // docs::/#tag
// Supports optional pause/resume:
// // docs::#tag-pause[:replacement]
// // docs::#tag-resume
function extractDocsTagBlock(fullText, tagWithHash) {
  const tag = tagWithHash.trim(); // e.g. "#foo"
  const startRe = new RegExp(`^\\s*//\\s*docs::\\s*${escapeRegex(tag)}\\s*$`, 'm');
  const endRe = new RegExp(`^\\s*//\\s*docs::/\\s*${escapeRegex(tag)}\\s*([)};]*)\\s*$`, 'm');

  const start = startRe.exec(fullText);
  if (!start) return { ok: false, content: `// Section '${tag}' not found` };
  const tail = fullText.slice(start.index + start[0].length);

  const end = endRe.exec(tail);
  if (!end) return { ok: false, content: `// Section '${tag}' end not found` };

  let body = tail.slice(0, end.index);
  const closers = end[1] || '';

  // pause/resume
  const pauseStartRe = new RegExp(`^\\s*//\\s*docs::\\s*${escapeRegex(tag)}-pause(?::(.*))?\\s*$`, 'm');
  const pauseResumeRe = new RegExp(`^\\s*//\\s*docs::\\s*${escapeRegex(tag)}-resume\\s*$`, 'm');
  while (true) {
    const p = pauseStartRe.exec(body);
    if (!p) break;
    const before = body.slice(0, p.index);
    const afterPause = body.slice(p.index + p[0].length);
    const r = pauseResumeRe.exec(afterPause);
    if (!r) {
      body = before + (p[1] || '');
      break;
    }
    const replacement = (p[1] || '').trim();
    const after = afterPause.slice(r.index + r[0].length);
    body = before + (replacement ? replacement : '') + after;
  }

  let cleaned = removeLeadingSpaces(body);
  if (closers) {
    const arr = [];
    for (let i = 0; i < closers.length; i++) {
      const c = closers[i];
      const next = closers[i + 1];
      if (next === ';') {
        arr.push(c + next);
        i++;
      } else {
        arr.push(c);
      }
    }
    for (let i = 0; i < arr.length; i++) {
      const spaces = '  '.repeat(arr.length - 1 - i);
      cleaned += `\n${spaces}${arr[i]}`;
    }
  }
  return { ok: true, content: cleaned };
}

// ---------- selector parsing ----------
function splitPathAndSelector(filePath) {
  const idx = filePath.indexOf('#');
  if (idx < 0) return { pathOnly: filePath, selector: null };
  return { pathOnly: filePath.slice(0, idx), selector: filePath.slice(idx) };
}
function getMarkerName(mark, key) {
  return mark && mark.includes(key)
    ? mark.substring(mark.indexOf(key) + key.length).trim()
    : null;
}

// ---------- extraction per selector ----------
function extractBySelector(src, selector, lang) {
  if (!selector) return src;

  const funKey = '#fun=';
  const structKey = '#struct=';
  const moduleKey = '#module=';
  const varKey = '#variable=';
  const useKey = '#use=';
  const componentKey = '#component=';
  const enumKey = '#enum=';
  const typeKey = '#type=';
  const traitKey = '#trait=';

  const isMove = lang === 'move';
  const isTs = lang === 'ts' || lang === 'js';
  const isRust = lang === 'rust';

  const funName = getMarkerName(selector, funKey);
  const structName = getMarkerName(selector, structKey);
  const moduleName = getMarkerName(selector, moduleKey);
  const variableName = getMarkerName(selector, varKey);
  const useName = getMarkerName(selector, useKey);
  const componentName = getMarkerName(selector, componentKey);
  const enumName = getMarkerName(selector, enumKey);
  const typeName = getMarkerName(selector, typeKey);
  const traitName = getMarkerName(selector, traitKey);

  function capPrepend(match) {
    // Preserve relative indent context by trimming uniformly
    return removeLeadingSpaces(match);
  }

  if (funName) {
    const names = funName.split(',').map((s) => s.trim()).filter(Boolean);
    const out = [];
    for (let fn of names) {
      let reStr = '';
      if (isMove) {
        reStr = `^(\\s*)*?(pub(lic)? )?(entry )?fu?n \\b${escapeRegex(fn)}\\b[\\s\\S]*?^\\}`;
      } else if (isTs) {
        reStr = `^(\\s*)(async )?(export (default )?)?function \\b${escapeRegex(fn)}\\b[\\s\\S]*?\\n\\1\\}`;
      } else if (isRust) {
        reStr = `^(\\s*)(pub\\s+)?(async\\s+)?(const\\s+)?(unsafe\\s+)?(extern\\s+("[^"]+"\\s*)?)?fn\\s+${escapeRegex(fn)}\\s*(<[^>]*>)?\\s*\\([^)]*\\)\\s*(->\\s*[^;{]+)?\\s*[;{][\\s\\S]*?^\\}`;
      } else {
        // generic
        reStr = `\\b${escapeRegex(fn)}\\b[\\s\\S]*?\\}`;
      }
      const re = new RegExp(reStr, 'ms');
      const m = re.exec(src);
      if (m) out.push(capPrepend(m[0]));
    }
    return out.join('\n\n').trim() || src;
  }

  if (structName) {
    const names = structName.split(',').map((s) => s.trim()).filter(Boolean);
    const out = [];
    for (let nm of names) {
      let re = new RegExp(`^(\\s*)\\b(pub(lic)?\\s+)?struct\\s+${escapeRegex(nm)}\\b;\\s*$`, 'm');
      let m = re.exec(src);
      if (!m) {
        re = new RegExp(`^(\\s*)*?(pub(lic)? )?struct \\b${escapeRegex(nm)}\\b[\\s\\S]*?^\\}`, 'ms');
        m = re.exec(src);
      }
      if (m) out.push(capPrepend(m[0]));
    }
    return out.join('\n\n').trim() || src;
  }

  if (traitName) {
    const names = traitName.split(',').map((s) => s.trim()).filter(Boolean);
    const out = [];
    for (let nm of names) {
      const re = new RegExp(`^(\\s*)*?(pub(lic)? )?trait \\b${escapeRegex(nm)}\\b[\\s\\S]*?^\\}`, 'ms');
      const m = re.exec(src);
      if (m) out.push(capPrepend(m[0]));
    }
    return out.join('\n\n').trim() || src;
  }

  if (variableName) {
    const names = variableName.split(',').map((s) => s.trim()).filter(Boolean);
    const out = [];
    if (isTs) {
      for (let v of names) {
        const reFun = new RegExp(`^( *)?.*?(let|const) \\b${escapeRegex(v)}\\b.*=>`, 'm');
        const reVar = new RegExp(`^( *)?.*?(let|const) \\b${escapeRegex(v)}\\b (?!.*=>)=.*;`, 'm');
        const mFun = reFun.exec(src);
        const mVar = reVar.exec(src);
        if (mFun) {
          const start = src.slice(mFun.index);
          const endRe = new RegExp(`^${mFun[1] ? mFun[1] : ''}\\)?\\};`, 'm');
          const end = endRe.exec(start);
          if (end) out.push(capPrepend(start.slice(0, end.index + end[0].length)));
        } else if (mVar) {
          out.push(capPrepend(mVar[0]));
        }
      }
      return out.join('\n\n').trim() || src;
    } else {
      for (let v of names) {
        const shortRe = new RegExp(
          `^(\\s*)?(#\\[test_only\\])?(let|const) \\(?.*?\\b${escapeRegex(v)}\\b.*?\\)?\\s?=.*;`,
          'm'
        );
        const longRe = new RegExp(
          `^(\\s*)?(#\\[test_only\\])?(let|const) \\(?.*?\\b${escapeRegex(v)}\\b.*?\\)?\\s?= \\{[^}]*\\};\\s*$`,
          'm'
        );
        const m = shortRe.exec(src) || longRe.exec(src);
        if (m) out.push(capPrepend(m[0]));
      }
      return out.join('\n\n').trim() || src;
    }
  }

  if (useName) {
    const uses = useName.split(',').map((s) => s.trim()).filter(Boolean);
    const out = [];
    for (let u of uses) {
      const [base, last] = u.split('::');
      const re = new RegExp(
        `^( *)(#\\[test_only\\] )?use ${escapeRegex(base)}::\\{?.*?${last ? escapeRegex(last) : ''}.*?\\};`,
        'ms'
      );
      const m = re.exec(src);
      if (m) out.push(capPrepend(m[0]));
    }
    return out.join('\n\n').trim() || src;
  }

  if (componentName && isTs) {
    const components = componentName.split(',').map((s) => s.trim()).filter(Boolean);
    const out = [];
    for (let comp of components) {
      let name = comp,
        element = '',
        ordinal = '';
      if (comp.includes(':')) {
        const parts = comp.split(':');
        name = parts[0];
        element = parts[1];
        ordinal = parts[2] || '';
      }
      const re = new RegExp(`^( *)(export (default )?)?function \\b${escapeRegex(name)}\\b[\\s\\S]*?\\n\\1\\}`, 'ms');
      const m = re.exec(src);
      if (m) {
        if (element) {
          const elRe = new RegExp(`^( *)\\<${escapeRegex(element)}\\b[\\s\\S]*?\\<\\/${escapeRegex(element)}\\>`, 'msg');
          let keep = [1];
          if (ordinal.includes('-') && !ordinal.includes('&')) {
            const [a, b] = ordinal.split('-').map(Number);
            keep = Array.from({ length: b - a + 1 }, (_, i) => a + i);
          } else if (ordinal.includes('&')) {
            keep = ordinal.split('&').map(Number);
          }
          keep.sort((a, b) => a - b);
          let count = 0;
          let match;
          while ((match = elRe.exec(m[0]))) {
            count++;
            if (keep.includes(count)) out.push(removeLeadingSpaces(match[0]));
          }
          if (out.length === 0) out.push(capPrepend(m[0]));
        } else {
          out.push(capPrepend(m[0]));
        }
      }
    }
    return out.join('\n\n').trim() || src;
  }

  if (moduleName && isMove) {
    const re = new RegExp(`^(\\s*)*module \\b${escapeRegex(moduleName)}\\b[\\s\\S]*?^\\}`, 'ms');
    const m = re.exec(src);
    if (m) return capPrepend(m[0]);
    return src;
  }

  if (enumName && isTs) {
    const enums = enumName.split(',').map((s) => s.trim()).filter(Boolean);
    const out = [];
    for (let e of enums) {
      const re = new RegExp(`^( *)(export)? enum \\b${escapeRegex(e)}\\b\\s*\\{[\\s\\S]*?\\}`, 'm');
      const m = re.exec(src);
      if (m) out.push(removeLeadingSpaces(m[0]));
    }
    return out.join('\n\n').trim() || src;
  }

  if (typeName && isTs) {
    const types = typeName.split(',').map((s) => s.trim()).filter(Boolean);
    const out = [];
    for (let t of types) {
      const startRe = new RegExp(`^( *)(export )?type \\b${escapeRegex(t)}\\b`, 'm');
      const m = startRe.exec(src);
      if (m) {
        let sub = src.slice(m.index);
        const spaces = m[1] || '';
        const endRe = new RegExp(`^${spaces}\\};`, 'm');
        const e = endRe.exec(sub);
        if (e) out.push(removeLeadingSpaces(sub.slice(0, e.index + e[0].length)));
      }
    }
    return out.join('\n\n').trim() || src;
  }

  // Fallback: docs-tag block (#tag)
  if (selector.startsWith('#')) {
    const { ok, content } = extractDocsTagBlock(src, selector);
    return ok ? content : content; // returns error string if not ok
  }

  return src;
}

export default function CodeFromFile({
  filePath,            // e.g. "crates/.../processor.rs#fun=process_block"
  language,            // optional override
  title,               // optional CodeBlock title
  repo = GITHUB_REPO,
  branch = GITHUB_BRANCH,
}) {
  const [{ code, err }, setState] = React.useState({ code: '', err: null });

  const { pathOnly, selector } = filePath ? splitPathAndSelector(filePath) : { pathOnly: '', selector: null };
  const lang = language || inferLanguage(pathOnly);
  const displayTitle = title ?? (filePath || 'code');

  React.useEffect(() => {
    let cancelled = false;
    async function load() {
      if (!pathOnly) return;
      const isHttp = /^https?:\/\//i.test(pathOnly);
      const url = isHttp
        ? pathOnly
        : `https://raw.githubusercontent.com/${repo}/${branch}/${pathOnly.replace(/^\.?\//, '')}`;
      try {
        const res = await fetch(url);
        if (!res.ok) throw new Error(`HTTP ${res.status} for ${url}`);
        const txt = await res.text();
        const extracted = extractBySelector(txt, selector, lang);
        if (!cancelled) setState({ code: extracted, err: null });
      } catch (e) {
        if (!cancelled) setState({ code: '', err: e?.message || 'Failed to load code' });
      }
    }
    load();
    return () => { cancelled = true; };
  }, [pathOnly, selector, repo, branch, lang]);

  if (!filePath) {
    return <em>Missing <code>filePath</code> for <code>&lt;CodeFromFile /&gt;</code>.</em>;
  }

  if (err) {
    return (
      <CodeBlock language="text" title={displayTitle}>
        {`// Failed to load "${filePath}"\n// ${err}`}
      </CodeBlock>
    );
  }

  if (!code) {
    return (
      <CodeBlock language="text" title={displayTitle}>
        {`// Loading ${filePath}...`}
      </CodeBlock>
    );
  }

  return (
    <CodeBlock language={lang} title={displayTitle}>
      {code}
    </CodeBlock>
  );
}
