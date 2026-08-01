import { describe, expect, it } from 'vitest';
import { addMarker, BUILT_IN_MARKERS, normalizeMarker } from './markers';

describe('special markers', () => {
  it('keeps the account marker built in without accounting behavior', () => {
    expect(BUILT_IN_MARKERS).toContain('账目');
    expect(BUILT_IN_MARKERS).not.toContain('收入');
    expect(BUILT_IN_MARKERS).not.toContain('支出');
  });

  it('normalizes, bounds, and deduplicates custom markers', () => {
    expect(normalizeMarker('  等待   回复  ')).toBe('等待 回复');
    expect(normalizeMarker('x'.repeat(80))).toHaveLength(40);
    expect(addMarker(['重要'], '重要')).toEqual(['重要']);
    expect(addMarker(['重要'], '  自定义  ')).toEqual(['重要', '自定义']);
  });
});
