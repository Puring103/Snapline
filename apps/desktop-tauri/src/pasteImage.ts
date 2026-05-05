interface ClipboardImageData {
  files?: Iterable<File>;
  items?: Iterable<{
    kind: string;
    type: string;
    getAsFile: () => File | null;
    getAsString?: (callback: (value: string) => void) => void;
  }>;
  getData?: (type: string) => string;
}

type PngEncoder = (file: Blob) => Promise<number[]>;

export type PastedImageSource =
  | { kind: "file"; file: File }
  | { kind: "local-file"; path: string; mimeType: string };

export function hasPotentialAsyncImageClipboardSource(clipboardData: ClipboardImageData): boolean {
  return Array.from(clipboardData.items ?? []).some((item) => {
    if (item.kind !== "string" || typeof item.getAsString !== "function") {
      return false;
    }

    return isClipboardTextTypeThatMayContainImage(item.type);
  });
}

export function pastedImageFileFromClipboard(clipboardData: ClipboardImageData): File | null {
  const source = pastedImageSourceFromClipboard(clipboardData);
  return source?.kind === "file" ? source.file : null;
}

export function pastedImageSourceFromClipboard(clipboardData: ClipboardImageData): PastedImageSource | null {
  const imageItem = Array.from(clipboardData.items ?? []).find(
    (item) => item.type.startsWith("image/") && typeof item.getAsFile === "function",
  );
  const itemFile = imageItem?.getAsFile();
  if (itemFile) {
    return { kind: "file", file: itemFile };
  }

  const file = Array.from(clipboardData.files ?? []).find((candidate) => candidate.type.startsWith("image/"));
  if (file) {
    return { kind: "file", file };
  }

  const htmlImage = dataUrlFromHtml(clipboardData.getData?.("text/html") ?? "");
  if (htmlImage) {
    const dataUrlFile = fileFromDataUrl(htmlImage, "clipboard-image.png");
    return dataUrlFile ? { kind: "file", file: dataUrlFile } : null;
  }

  const localFile = localImageFileFromClipboard(clipboardData);
  return localFile ? { kind: "local-file", ...localFile } : null;
}

export async function pastedImageSourceFromClipboardAsync(
  clipboardData: ClipboardImageData,
): Promise<PastedImageSource | null> {
  const immediate = pastedImageSourceFromClipboard(clipboardData);
  if (immediate) {
    return immediate;
  }

  const stringItems = Array.from(clipboardData.items ?? []).filter(
    (item) =>
      item.kind === "string"
      && typeof item.getAsString === "function"
      && isClipboardTextTypeThatMayContainImage(item.type),
  );
  for (const item of stringItems) {
    const value = await stringFromClipboardItem(item);
    const localFile = localImageFileFromClipboardText(item.type, value);
    if (localFile) {
      return { kind: "local-file", ...localFile };
    }
  }

  return null;
}

export async function bytesFromPastedImageFile(
  file: Blob,
  pngEncoder: PngEncoder = bytesFromImageBlobAsPng,
): Promise<number[]> {
  if (!isPngImage(file)) {
    return pngEncoder(file);
  }

  return bytesFromArrayBuffer(await arrayBufferFromBlob(file));
}

export function objectUrlFromImageBytes(
  bytes: number[],
  mimeType: string,
  createObjectUrl: (blob: Blob) => string = URL.createObjectURL,
): string {
  return createObjectUrl(new Blob([new Uint8Array(bytes)], { type: mimeType }));
}
export async function bytesFromTransientImageSource(source: string): Promise<number[]> {
  if (source.startsWith("data:")) {
    return bytesFromDataUrl(source);
  }

  const response = await fetch(source);
  if (!response.ok) {
    throw new Error(`Unable to read pasted image: ${response.status}`);
  }

  return bytesFromArrayBuffer(await response.arrayBuffer());
}

function bytesFromDataUrl(source: string): number[] {
  const commaIndex = source.indexOf(",");
  if (commaIndex === -1) {
    throw new Error("Unable to read pasted image: invalid data url");
  }

  const metadata = source.slice(0, commaIndex);
  const payload = source.slice(commaIndex + 1);
  if (!metadata.includes(";base64")) {
    return bytesFromString(decodeURIComponent(payload));
  }

  return bytesFromString(atob(payload));
}

function bytesFromString(value: string): number[] {
  return Array.from(value, (character) => character.charCodeAt(0));
}

function dataUrlFromHtml(html: string): string | null {
  const match = html.match(/<img[^>]+src=["']([^"']+)["']/i);
  if (!match) {
    return null;
  }

  return match[1].startsWith("data:image/") ? match[1] : null;
}

function localImageFileFromClipboard(
  clipboardData: ClipboardImageData,
): { path: string; mimeType: string } | null {
  const uriList = clipboardData.getData?.("text/uri-list") ?? "";
  const uriFromList = firstFileUriFromText(uriList);
  const htmlUri = fileUrlFromHtml(clipboardData.getData?.("text/html") ?? "");
  const plainTextUri = firstFileUriFromText(clipboardData.getData?.("text/plain") ?? "");
  const uri = uriFromList ?? htmlUri ?? plainTextUri;
  return localImageFileFromUri(uri);
}

function localImageFileFromClipboardText(
  type: string,
  value: string,
): { path: string; mimeType: string } | null {
  const normalizedType = type.toLowerCase();
  if (normalizedType === "text/html") {
    return localImageFileFromUri(fileUrlFromHtml(value));
  }

  if (
    normalizedType === "text/uri-list"
    || normalizedType === "x-special/gnome-copied-files"
    || normalizedType === "text/plain"
  ) {
    return localImageFileFromUri(firstFileUriFromText(value));
  }

  return null;
}

function isClipboardTextTypeThatMayContainImage(type: string): boolean {
  const normalizedType = type.toLowerCase();
  return (
    normalizedType === "text/html"
    || normalizedType === "text/uri-list"
    || normalizedType === "x-special/gnome-copied-files"
    || normalizedType === "text/plain"
  );
}

function firstFileUriFromText(value: string): string | null {
  return value
    .split(/\r?\n/)
    .map((line) => line.trim())
    .find((line) => line.startsWith("file://")) ?? null;
}

function localImageFileFromUri(uri: string | null | undefined): { path: string; mimeType: string } | null {
  if (!uri?.startsWith("file://")) {
    return null;
  }

  const path = pathFromFileUri(uri);
  if (!path) {
    return null;
  }

  const mimeType = imageMimeTypeFromPath(path);
  return mimeType ? { path, mimeType } : null;
}

function stringFromClipboardItem(item: {
  getAsString?: (callback: (value: string) => void) => void;
}): Promise<string> {
  return new Promise((resolve) => {
    item.getAsString?.((value) => resolve(value));
  });
}

function fileUrlFromHtml(html: string): string | null {
  const match = html.match(/<img[^>]+src=["']([^"']+)["']/i);
  return match?.[1].startsWith("file://") ? match[1] : null;
}

function pathFromFileUri(uri: string): string | null {
  try {
    const url = new URL(uri);
    if (url.protocol !== "file:") {
      return null;
    }

    return decodeURIComponent(url.pathname);
  } catch {
    return null;
  }
}

function imageMimeTypeFromPath(path: string): string | null {
  const extension = path.split(".").pop()?.toLowerCase();
  switch (extension) {
    case "png":
      return "image/png";
    case "jpg":
    case "jpeg":
      return "image/jpeg";
    case "webp":
      return "image/webp";
    case "gif":
      return "image/gif";
    case "bmp":
      return "image/bmp";
    default:
      return null;
  }
}
function fileFromDataUrl(source: string, name: string): File | null {
  const commaIndex = source.indexOf(",");
  if (commaIndex === -1) {
    return null;
  }

  const metadata = source.slice(0, commaIndex);
  const payload = source.slice(commaIndex + 1);
  const mimeType = metadata.slice(5, metadata.indexOf(";") === -1 ? undefined : metadata.indexOf(";"));
  const bytes = metadata.includes(";base64")
    ? Uint8Array.from(atob(payload), (character) => character.charCodeAt(0))
    : Uint8Array.from(decodeURIComponent(payload), (character) => character.charCodeAt(0));

  return new File([bytes], name, { type: mimeType || "image/png" });
}

function isPngImage(file: Blob): boolean {
  return file.type.toLowerCase() === "image/png";
}

async function bytesFromImageBlobAsPng(file: Blob): Promise<number[]> {
  const canvas = await canvasFromImageBlob(file);

  const pngBlob = await new Promise<Blob>((resolve, reject) => {
    if (!canvas.toBlob) {
      reject(new Error("Unable to read pasted image"));
      return;
    }

    canvas.toBlob((result) => {
      if (result) {
        resolve(result);
        return;
      }
      reject(new Error("Unable to read pasted image"));
    }, "image/png");
  });

  return bytesFromArrayBuffer(await arrayBufferFromBlob(pngBlob));
}

async function canvasFromImageBlob(blob: Blob): Promise<HTMLCanvasElement> {
  const canvas = document.createElement("canvas");
  const image = new Image();
  const url = URL.createObjectURL(blob);

  try {
    const loaded = new Promise<void>((resolve, reject) => {
      image.onload = () => resolve();
      image.onerror = () => reject(new Error("Unable to read pasted image"));
    });
    image.src = url;
    await loaded;
    canvas.width = image.naturalWidth;
    canvas.height = image.naturalHeight;
    const context = canvas.getContext("2d");
    if (!context) {
      throw new Error("Unable to read pasted image");
    }
    context.drawImage(image, 0, 0);
    return canvas;
  } finally {
    URL.revokeObjectURL(url);
  }
}

function bytesFromArrayBuffer(buffer: ArrayBuffer): number[] {
  return Array.from(new Uint8Array(buffer));
}

function arrayBufferFromBlob(blob: Blob): Promise<ArrayBuffer> {
  if ("arrayBuffer" in blob && typeof blob.arrayBuffer === "function") {
    return blob.arrayBuffer();
  }

  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onerror = () => reject(reader.error ?? new Error("Unable to read pasted image"));
    reader.onload = () => {
      if (reader.result instanceof ArrayBuffer) {
        resolve(reader.result);
      } else {
        reject(new Error("Unable to read pasted image"));
      }
    };
    reader.readAsArrayBuffer(blob);
  });
}
