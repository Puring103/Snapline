export function blobUrlFromBytes(
  bytes: number[],
  createObjectUrl: (blob: Blob) => string = URL.createObjectURL,
): string {
  return createObjectUrl(new Blob([new Uint8Array(bytes)], { type: "image/png" }));
}
