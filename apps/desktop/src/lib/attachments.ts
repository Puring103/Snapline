import type { SourceType } from '../types';

export interface AttachmentReference {
  id: string;
  displayName: string;
  kind: 'image' | 'audio' | 'video' | 'file';
  url: string;
}

const attachmentPattern = /(!?)\[([^\]]*)\]\(snapline-attachment:\/\/(?:localhost\/)?([0-9a-f-]{36})(?:\/([^\s)]+))?\)/gi;

function kindFromName(displayName: string, imageSyntax: boolean, fallback?: SourceType): AttachmentReference['kind'] {
  const extension = displayName.split('.').pop()?.toLocaleLowerCase();
  if (['png', 'jpg', 'jpeg', 'webp', 'gif'].includes(extension || '') || imageSyntax) return 'image';
  if (extension === 'wav' || fallback === 'audio') return 'audio';
  if (['mp4', 'mov', 'webm', 'mkv'].includes(extension || '') || fallback === 'video') return 'video';
  return 'file';
}

export function attachmentUrl(id: string, displayName: string) {
  const safeName = displayName.trim() || '附件';
  return `snapline-attachment://localhost/${id}/${encodeURIComponent(safeName)}`;
}

export function attachmentMarkdown(id: string, displayName: string, type: SourceType) {
  const label = displayName.trim() || '附件';
  const reference = `[${label}](${attachmentUrl(id, label)})`;
  return type === 'image' || type === 'screenshot' ? `!${reference}` : reference;
}

export function parseAttachmentReferences(markdown: string, fallback?: SourceType): AttachmentReference[] {
  return [...markdown.matchAll(attachmentPattern)].map((match) => {
    const displayName = match[4] ? decodeURIComponent(match[4]) : match[2] || '附件';
    return {
      id: match[3],
      displayName,
      kind: kindFromName(displayName, Boolean(match[1]), fallback),
      url: attachmentUrl(match[3], displayName),
    };
  });
}
