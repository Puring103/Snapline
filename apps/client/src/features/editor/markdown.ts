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

export function titleFromMarkdownViaRust(markdown: string): Promise<string> {
  return api.deriveTitleFromMarkdown(markdown);
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

function assetUrlFromMarkdownPath(markdownPath: string): string {
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

export function composeDraftMarkdownViaRust(title: string, bodyMd: string): Promise<string> {
  return api.composeDraftMarkdown(title, bodyMd);
}

export function splitDraftMarkdownViaRust(markdown: string): Promise<{ title: string; body_md: string }> {
  return api.splitDraftMarkdown(markdown);
}

export function splitStoredNoteMarkdownViaRust(
  storedTitle: string,
  markdown: string,
): Promise<{ title: string; body_md: string }> {
  return api.splitStoredNoteMarkdown(storedTitle, markdown);
}
