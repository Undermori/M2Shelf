/** Viewport coordinates for a measured popup; no assumption about translated row heights. */
export function floatingPanelPosition(anchor: Pick<DOMRect, 'left' | 'right' | 'top' | 'bottom'>, size: {width: number; height: number}, viewport: {width: number; height: number}, align: 'start' | 'end' = 'start', gap = 6, edge = 12) {
  const width = Math.max(0, Math.min(size.width, viewport.width - edge * 2));
  const below = Math.max(0, viewport.height - anchor.bottom - edge - gap);
  const above = Math.max(0, anchor.top - edge - gap);
  const downward = below >= size.height || below >= above;
  const maxHeight = Math.max(0, Math.min(size.height, viewport.height - edge * 2, downward ? below : above));
  const left = align === 'end' ? anchor.right - width : anchor.left;
  const top = downward ? anchor.bottom + gap : anchor.top - maxHeight - gap;
  return {left: Math.max(edge, Math.min(left, viewport.width - width - edge)), top: Math.max(edge, Math.min(top, viewport.height - maxHeight - edge)), width, maxHeight};
}
