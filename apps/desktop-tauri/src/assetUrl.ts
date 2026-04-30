import { convertFileSrc } from "@tauri-apps/api/core";

export function webviewAssetUrlFromFilesystemPath(
  filesystemPath: string,
  convert = convertFileSrc,
): string {
  return convert(filesystemPath);
}

export function webviewAssetUrlFromMarkdownPath(
  dataDir: string,
  markdownPath: string,
  convert = convertFileSrc,
): string {
  if (!markdownPath.startsWith("assets/")) {
    return markdownPath;
  }

  return convert(joinDataDirPath(dataDir, markdownPath));
}

export function fileUrlFromMarkdownPath(dataDir: string, markdownPath: string): string {
  if (!markdownPath.startsWith("assets/")) {
    return markdownPath;
  }

  return `file:///${joinDataDirPath(dataDir, markdownPath).replace(/\\/g, "/")}`;
}

function joinDataDirPath(dataDir: string, markdownPath: string): string {
  const normalizedDataDir = dataDir.replace(/[/\\]+$/, "");
  const normalizedMarkdownPath = markdownPath.replace(/^[/\\]+/, "").replace(/\//g, "\\");
  return `${normalizedDataDir}\\${normalizedMarkdownPath}`;
}
