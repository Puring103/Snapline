import { FileLock2 } from 'lucide-react';
import { parseAttachmentReferences } from '../lib/attachments';
import { isTauri } from '../lib/native';
import type { Item } from '../types';

export function AttachmentPreview({ item }: { item: Item }) {
  if (!isTauri() || !item.content.attachment_ids.length) return null;
  const fallback = item.content.attachment_ids.length === 1 ? item.content.source_type : undefined;
  const references = parseAttachmentReferences(item.content.markdown, fallback);
  if (!references.length) return null;

  return <section className="attachment-preview" aria-label="加密附件">
    {references.map((attachment) => <figure key={attachment.id}>
      {attachment.kind === 'image' && <img src={attachment.url} alt={attachment.displayName} />}
      {attachment.kind === 'audio' && <audio src={attachment.url} controls preload="metadata" />}
      {attachment.kind === 'video' && <video src={attachment.url} controls preload="metadata" />}
      {attachment.kind === 'file' && <div className="encrypted-file"><FileLock2 size={18} /><span>{attachment.displayName}</span></div>}
      {attachment.kind !== 'file' && <figcaption>{attachment.displayName}</figcaption>}
    </figure>)}
  </section>;
}
