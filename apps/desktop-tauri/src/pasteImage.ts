interface ClipboardImageData {
  files?: Iterable<File>;
  items?: Iterable<{
    kind: string;
    type: string;
    getAsFile: () => File | null;
  }>;
}

export function pastedImageFileFromClipboard(clipboardData: ClipboardImageData): File | null {
  const imageItem = Array.from(clipboardData.items ?? []).find(
    (item) => item.kind === "file" && item.type.startsWith("image/"),
  );
  const itemFile = imageItem?.getAsFile();
  if (itemFile) {
    return itemFile;
  }

  return Array.from(clipboardData.files ?? []).find((file) => file.type.startsWith("image/")) ?? null;
}

export async function bytesFromPastedImageFile(file: Blob): Promise<number[]> {
  return bytesFromArrayBuffer(await arrayBufferFromBlob(file));
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
