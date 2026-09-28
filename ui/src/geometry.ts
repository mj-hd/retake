import type { Selection } from './types.ts';

export interface Point {
  x: number;
  y: number;
}

export interface Size {
  width: number;
  height: number;
}

export interface Rect extends Point, Size {}

export function clamp(value: number, low: number, high: number): number {
  return Math.min(high, Math.max(low, value));
}

export function clampZoom(zoom: number, minimum = 0.25, maximum = 4): number {
  return clamp(zoom, minimum, maximum);
}

export function rectFromPoints(start: Point, end: Point): Rect {
  return {
    x: Math.min(start.x, end.x),
    y: Math.min(start.y, end.y),
    width: Math.abs(end.x - start.x),
    height: Math.abs(end.y - start.y),
  };
}

export function selectionFromPoints(start: Point, end: Point, threshold = 4): Selection {
  const rect = rectFromPoints(start, end);
  return rect.width < threshold && rect.height < threshold ? { kind: 'point', ...start } : { kind: 'rect', ...rect };
}

export function pointInImage(client: Point, bounds: Rect, image: Size): Point {
  return {
    x: clamp(((client.x - bounds.x) * image.width) / bounds.width, 0, image.width),
    y: clamp(((client.y - bounds.y) * image.height) / bounds.height, 0, image.height),
  };
}

export function percentageInBounds(clientX: number, bounds: Pick<Rect, 'x' | 'width'>): number {
  return clamp(((clientX - bounds.x) / bounds.width) * 100, 0, 100);
}

export function selectionPlacement(selection: Selection, size: Size) {
  const width = selection.kind === 'rect' ? (selection.width ?? 0) : 0;
  const height = selection.kind === 'rect' ? (selection.height ?? 0) : 0;
  const x = selection.x + width;
  const y = selection.y + height;
  return { x, y, edgeRight: x / size.width > 0.65, edgeBottom: y / size.height > 0.7 };
}

export function percentageRect(rect: Rect, size: Size) {
  return {
    left: `${(rect.x / size.width) * 100}%`,
    top: `${(rect.y / size.height) * 100}%`,
    width: `${(rect.width / size.width) * 100}%`,
    height: `${(rect.height / size.height) * 100}%`,
  };
}

export function movedBeyond(start: Point, end: Point, threshold = 4): boolean {
  return Math.abs(end.x - start.x) + Math.abs(end.y - start.y) > threshold;
}
