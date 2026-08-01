export const BUILT_IN_MARKERS = ['账目', '重要', '待办', '稍后处理', '需跟进'] as const;

export function normalizeMarker(value: string) {
  return value.trim().replace(/\s+/g, ' ').slice(0, 40);
}

export function addMarker(markers: string[], value: string) {
  const marker = normalizeMarker(value);
  return marker && !markers.includes(marker) ? [...markers, marker] : markers;
}
