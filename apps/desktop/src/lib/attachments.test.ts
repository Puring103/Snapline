import { describe, expect, it } from 'vitest';
import { attachmentMarkdown, parseAttachmentReferences } from './attachments';

const id = 'd9d88986-050b-40ce-95ec-f4401b612b5b';

describe('encrypted attachment references', () => {
  it('round-trips encoded image, audio, and video names', () => {
    const markdown = [
      attachmentMarkdown(id, '粘贴 图片.png', 'image'),
      attachmentMarkdown(id, '录音.wav', 'audio'),
      attachmentMarkdown(id, '视频.mp4', 'video'),
    ].join('\n');
    const references = parseAttachmentReferences(markdown);
    expect(references.map((reference) => reference.kind)).toEqual(['image', 'audio', 'video']);
    expect(references[0].displayName).toBe('粘贴 图片.png');
    expect(references[0].url).toContain(encodeURIComponent('粘贴 图片.png'));
  });

  it('reads legacy references and uses the record type when no extension exists', () => {
    const [reference] = parseAttachmentReferences(`[录音](snapline-attachment://${id})`, 'audio');
    expect(reference.kind).toBe('audio');
    expect(reference.id).toBe(id);
  });
});
