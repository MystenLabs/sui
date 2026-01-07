
const { decode } = require("he");

export function truncateAtWord(text, maxChars = 250) {
  if (text.length <= maxChars) return text;
  const decoded = decode(text);
  const truncated = decoded.slice(0, maxChars);
  return truncated.slice(0, truncated.lastIndexOf(" ")) + "…";
}

export function getDeepestHierarchyLabel(hierarchy) {
  const levels = ["lvl0", "lvl1", "lvl2", "lvl3", "lvl4", "lvl5", "lvl6"];
  let lastValue = null;

  for (const lvl of levels) {
    const value = hierarchy[lvl];
    if (value == null) {
      break;
    }
    lastValue = value;
  }

  return lastValue || hierarchy.lvl6 || "";
}
