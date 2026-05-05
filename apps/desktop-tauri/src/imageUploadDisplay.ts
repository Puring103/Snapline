export function uploadedImageDisplaySource(originalSource: string, assetUrl: string): string {
  return isTransientImageSource(originalSource) ? originalSource : assetUrl;
}

export function isTransientImageSource(source: unknown): source is string {
  return typeof source === "string" && (source.startsWith("blob:") || source.startsWith("data:"));
}
