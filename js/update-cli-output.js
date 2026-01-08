

const fs = require("fs");
const path = require("path");
const { execSync } = require("child_process");

console.log("🚀 Starting CLI documentation update batch...");
console.log("Current working directory:", process.cwd());

/**
 * Commands and their corresponding output files. These filenames are chosen
 * to mirror the workflow outputs (snippets/console-output/*.mdx).
 */
const COMMANDS = [
  { cmd: "sui client --help", out: "sui-client-help.mdx" },
  { cmd: "sui replay --help", out: "sui-replay-help.mdx" },
  { cmd: "sui keytool --help", out: "sui-keytool-help.mdx" },
  { cmd: "sui keytool sign --help", out: "sui-keytool-sign-help.mdx" },
  { cmd: "sui move --help", out: "sui-move-help.mdx" },
  { cmd: "sui move build --help", out: "sui-move-build-help.mdx" },
  { cmd: "sui validator --help", out: "sui-validator-help.mdx" },
  {
    cmd: "sui validator report-validator --help",
    out: "sui-validator-report-validator-help.mdx",
  },
  { cmd: "sui client call --help", out: "sui-client-call-help.mdx" },
  { cmd: "sui client ptb --help", out: "sui-client-ptb-help.mdx" },
];

/** Root for snippet outputs (kept identical to the workflow paths). */
const SNIPPETS_DIR = path.join(
  __dirname,
  "../../../content/snippets/console-output",
);

function ensureDir(p) {
  if (!fs.existsSync(p)) {
    fs.mkdirSync(p, { recursive: true });
  }
}

function runAndWrite({ cmd, out }) {
  console.log("\n📋 Running:", cmd);
  const started = Date.now();
  try {
    const output = execSync(cmd, { encoding: "utf8", timeout: 30_000 });
    const fenced = `\n\`\`\`sh\n${output.trim()}\n\`\`\`\n`;

    const target = path.join(SNIPPETS_DIR, out);
    fs.writeFileSync(target, fenced);

    console.log(
      `✅ Wrote ${out} (${fenced.length.toLocaleString()} bytes) in ${Date.now() - started}ms`,
    );
  } catch (err) {
    console.error("❌ Failed:", cmd);
    if (err.stdout || err.stderr) {
      const details = [
        err.stdout ? `STDOUT:\n${err.stdout}` : null,
        err.stderr ? `STDERR:\n${err.stderr}` : null,
      ]
        .filter(Boolean)
        .join("\n\n");
      console.error(details);
    } else {
      console.error(err);
    }

    // Still write an error stub so downstream pages have content.
    const target = path.join(SNIPPETS_DIR, out);
    const stub = `\n\`\`\`sh\n[error] Command failed: ${cmd}\n${(err && err.message) || ""}\n\`\`\`\n`;
    try {
      fs.writeFileSync(target, stub);
      console.log(`⚠️  Wrote error stub to ${out}`);
    } catch (writeErr) {
      console.error("❌ Also failed to write stub:", writeErr);
    }
  }
}

function main() {
  console.log("🛠  Ensuring snippets directory exists:", SNIPPETS_DIR);
  ensureDir(SNIPPETS_DIR);

  let ok = 0;
  let fail = 0;

  for (const item of COMMANDS) {
    try {
      runAndWrite(item);
      ok++;
    } catch {
      fail++;
    }
  }

  console.log(`\n🏁 Done. Success: ${ok}, Failed: ${fail}`);
}

main();
