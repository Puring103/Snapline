import { api } from "../../platform/api";

export function normalizeMarkdown(markdown: string): string {
  return markdown.replace(/\r\n/g, "\n").trimEnd();
}

export function imageMarkdown(path: string): string {
  return `![](${path})`;
}

export function markdownTextFromClipboard(clipboardData: { getData: (type: string) => string }): string {
  const markdown = clipboardData.getData("text/markdown").trim();
  if (markdown.length > 0) {
    return markdown;
  }

  const plainText = clipboardData.getData("text/plain").trim();
  return plainText;
}

export function titleFromMarkdown(markdown: string): string {
  const firstHeading = normalizeMarkdown(markdown)
    .split("\n")
    .map((line) => line.trim())
    .find((line) => line.startsWith("# "));

  if (!firstHeading) {
    return "Untitled";
  }

  const strippedHeading = firstHeading.replace(/^#\s+/, "").trim();
  return strippedHeading.length > 0 ? strippedHeading : "Untitled";
}

export function titleFromMarkdownViaRust(markdown: string): Promise<string> {
  return api.deriveTitleFromMarkdown(markdown);
}

export function previewFromMarkdown(markdown: string): string {
  return normalizeMarkdown(markdown)
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line.length > 0 && !line.startsWith("# "))
    .map((line) => line.replace(/^(-|\*|\d+\.)\s+/, "").trim())
    .filter(Boolean)
    .join("\n")
    .slice(0, 500)
    .trim();
}

export function previewMarkdownFromMarkdown(markdown: string): string {
  const lines = normalizeMarkdown(markdown).split("\n");
  const titleLineIndex = lines.findIndex((line) => line.trim().startsWith("# "));
  const previewLines = titleLineIndex === -1
    ? lines
    : lines.filter((_line, index) => index !== titleLineIndex);

  return previewLines.join("\n").trim();
}

export function replaceMarkdownImageSource(
  markdown: string,
  currentSource: string,
  nextSource: string,
): string {
  return rewriteMarkdownImageSources(markdown, (source) =>
    source === currentSource ? nextSource : source,
  );
}

export function hydrateMarkdownForEditor(markdown: string) {
  const sourceMap = new Map<string, string>();
  const imageSources = Array.from(markdown.matchAll(/!\[[^\]]*]\(([^)]+)\)/g))
    .map((match) => match[1])
    .filter((source) => source.startsWith("assets/"));

  let hydrated = markdown;
  for (const markdownPath of new Set(imageSources)) {
    const editorSource = assetUrlFromMarkdownPath(markdownPath);
    hydrated = replaceMarkdownImageSource(hydrated, markdownPath, editorSource);
    sourceMap.set(editorSource, markdownPath);
  }

  return { markdown: hydrated, sourceMap };
}

export function restoreMarkdownForStorage(
  markdown: string,
  sourceMap: Map<string, string>,
) {
  let restored = markdown;
  for (const [editorSource, markdownPath] of sourceMap) {
    restored = replaceMarkdownImageSource(restored, editorSource, markdownPath);
  }
  return restored;
}

export function rewriteMarkdownImageSources(
  markdown: string,
  transform: (source: string) => string,
): string {
  const imagePattern = /(!\[[^\]]*]\()([^)]+)(\))/g;
  return markdown.replace(imagePattern, (_match, prefix, source, suffix) => {
    return `${prefix}${transform(source)}${suffix}`;
  });
}

export function assetUrlFromMarkdownPath(markdownPath: string): string {
  if (!markdownPath.startsWith("assets/")) {
    return markdownPath;
  }

  return `asset://localhost/${markdownPath}`;
}

export function assetUrlFromMarkdownPathViaRust(markdownPath: string): Promise<string> {
  return api.assetUrlFromMarkdownPath(markdownPath);
}

export function markdownPathFromAssetUrl(assetUrl: string): string {
  const prefix = "asset://localhost/";
  return assetUrl.startsWith(prefix) ? assetUrl.slice(prefix.length) : assetUrl;
}

export function markdownPathFromAssetUrlViaRust(assetUrl: string): Promise<string> {
  return api.markdownPathFromAssetUrl(assetUrl);
}

export function codeBlockLanguageClass(language: string | null | undefined): string | null {
  const normalized = (language ?? "")
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9-]+/g, "-")
    .replace(/^-+|-+$/g, "");

  return normalized.length > 0 ? `language-${normalized}` : null;
}

export function hasTransientImageSource(markdown: string): boolean {
  return /!\[[^\]]*]\((blob:|data:)/.test(markdown);
}

export function stripTransientImageSources(markdown: string): string {
  return markdown.replace(/!\[[^\]]*]\((blob:|data:)[^)]+\)/g, "");
}

export function composeDraftMarkdown(title: string, bodyMd: string): string {
  const safeTitle = normalizeTitle(title);
  const normalizedBody = normalizeMarkdown(bodyMd);

  if (normalizedBody.length === 0) {
    return `# ${safeTitle}`;
  }

  return [`# ${safeTitle}`, "", normalizedBody].join("\n");
}

export function composeDraftMarkdownViaRust(title: string, bodyMd: string): Promise<string> {
  return api.composeDraftMarkdown(title, bodyMd);
}

export function splitDraftMarkdown(markdown: string): { title: string; body_md: string } {
  const normalized = normalizeMarkdown(markdown);
  const lines = normalized.split("\n");
  const firstVisibleLineIndex = lines.findIndex((line) => line.trim().length > 0);

  if (firstVisibleLineIndex === -1) {
    return { title: "Untitled", body_md: "" };
  }

  const title = normalizeTitle(lines[firstVisibleLineIndex]);
  const bodyLines = [
    ...lines.slice(0, firstVisibleLineIndex),
    ...lines.slice(firstVisibleLineIndex + 1),
  ];

  if (bodyLines[0]?.trim().length === 0) {
    bodyLines.shift();
  }

  return {
    title,
    body_md: normalizeMarkdown(bodyLines.join("\n")),
  };
}

export function splitDraftMarkdownViaRust(markdown: string): Promise<{ title: string; body_md: string }> {
  return api.splitDraftMarkdown(markdown);
}

export function splitStoredNoteMarkdown(
  storedTitle: string,
  markdown: string,
): { title: string; body_md: string } {
  const normalizedTitle = normalizeTitle(storedTitle);
  const normalizedMarkdown = normalizeMarkdown(markdown);

  if (normalizedMarkdown.length === 0) {
    return { title: normalizedTitle, body_md: "" };
  }

  const lines = normalizedMarkdown.split("\n");
  const firstVisibleLineIndex = lines.findIndex((line) => line.trim().length > 0);

  if (firstVisibleLineIndex === -1) {
    return { title: normalizedTitle, body_md: "" };
  }

  const firstVisibleTitle = normalizeTitle(lines[firstVisibleLineIndex]);
  if (firstVisibleTitle !== normalizedTitle) {
    return { title: normalizedTitle, body_md: normalizedMarkdown };
  }

  const bodyLines = [
    ...lines.slice(0, firstVisibleLineIndex),
    ...lines.slice(firstVisibleLineIndex + 1),
  ];

  if (bodyLines[0]?.trim().length === 0) {
    bodyLines.shift();
  }

  return {
    title: normalizedTitle,
    body_md: normalizeMarkdown(bodyLines.join("\n")),
  };
}

function normalizeTitle(title: string): string {
  const trimmed = normalizeMarkdown(title).trim();
  if (trimmed.length === 0) {
    return "Untitled";
  }

  return trimmed.replace(/^#+\s*/, "").trim() || "Untitled";
}
