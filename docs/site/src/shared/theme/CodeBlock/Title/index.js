
const GITHUB = "https://github.com";
const GITHUB_BLOB = "blob/main";
const ML_ORG = "MystenLabs";
const SUI_REPO = "sui";

export default function CodeBlockTitle({ children }) {
  let link;
  if (typeof children === "string" && children.startsWith("github.com/")) {
    const parts = children.split("/");
    const org = parts[1];
    const repo = parts[2];
    const rest = parts.slice(3).join("/");
    link = `${GITHUB}/${org}/${repo}/${GITHUB_BLOB}/${rest}`;
  } else {
    link = `${GITHUB}/${ML_ORG}/${SUI_REPO}/${GITHUB_BLOB}/${children}`;
  }
  return (
    <a href={link} target="_blank" rel="noreferrer">
      {children}
    </a>
  );
}
