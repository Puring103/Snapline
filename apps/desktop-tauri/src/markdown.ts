export function normalizeMarkdown(markdown: string): string {
  return markdown.replace(/\r\n/g, "\n").trimEnd();
}

export function imageMarkdown(path: string): string {
  return `![](${path})`;
}

export function titleFromMarkdown(markdown: string): string {
  const firstVisibleLine = normalizeMarkdown(markdown)
    .split("\n")
    .map((line) => line.trim())
    .find((line) => line.length > 0);

  if (!firstVisibleLine) {
    return "Untitled";
  }

  const strippedHeading = firstVisibleLine.replace(/^#+\s*/, "").trim();
  return strippedHeading.length > 0 ? strippedHeading : "Untitled";
}

export function replaceMarkdownImageSource(
  markdown: string,
  currentSource: string,
  nextSource: string,
): string {
  const imagePattern = new RegExp(
    `(!\\[[^\\]]*\\]\\()${escapeRegExp(currentSource)}(\\))`,
    "g",
  );
  return markdown.replace(imagePattern, `$1${nextSource}$2`);
}

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}
