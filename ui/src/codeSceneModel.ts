import { clamp, clampZoom, rectFromPoints } from './geometry.ts';
import type { Point, Rect, Size } from './geometry.ts';
import type { SceneData, SceneFrame, SceneSelection, SceneSymbol } from './CodeSceneTypes.ts';

export type SceneEdge = readonly [number, number, number, number];

export interface SceneLabel {
  key: string;
  x: number;
  y: number;
  text: string;
  frame: boolean;
  big: boolean;
  scale: number;
}

export function sceneViewport(zoom: number, pan: Point, viewSize: Size): Rect {
  return {
    x: -pan.x / zoom,
    y: -pan.y / zoom,
    width: (viewSize.width || 1000) / zoom,
    height: (viewSize.height || 700) / zoom,
  };
}

export function intersects(view: Rect, rect: Rect): boolean {
  return !(
    rect.x + rect.width < view.x ||
    rect.x > view.x + view.width ||
    rect.y + rect.height < view.y ||
    rect.y > view.y + view.height
  );
}

export function sceneTier(zoom: number): 0 | 1 | 2 {
  return zoom < 0.3 ? 0 : zoom < 0.5 ? 1 : 2;
}

function center(rect: Rect): Point {
  return { x: rect.x + rect.width / 2, y: rect.y + rect.height / 2 };
}

function pointOnFrame(frame: SceneFrame, point: Point): Point {
  return {
    x: clamp(point.x, frame.x, frame.x + frame.width),
    y: clamp(point.y, frame.y, frame.y + frame.height),
  };
}

function edgeBetween(start: Point, end: Point): SceneEdge {
  return [start.x, start.y, end.x, end.y];
}

function edgeBounds(edge: SceneEdge): Rect {
  return rectFromPoints({ x: edge[0], y: edge[1] }, { x: edge[2], y: edge[3] });
}

function uniqueLabels(candidates: SceneLabel[], minimumDistance: number): SceneLabel[] {
  const labels: SceneLabel[] = [];
  for (const candidate of candidates) {
    if (
      !labels.some(
        (label) =>
          Math.abs(label.x - candidate.x) < minimumDistance && Math.abs(label.y - candidate.y) < minimumDistance,
      )
    ) {
      labels.push(candidate);
    }
  }
  return labels;
}

export function deriveSceneCanvas(scene: SceneData, zoom: number, pan: Point, viewSize: Size) {
  const view = sceneViewport(zoom, pan, viewSize);
  const visible = <T extends Rect>(items: T[]) => items.filter((item) => intersects(view, item));
  const tier = sceneTier(zoom);
  const symbols = visible(scene.symbols);
  const imports = visible(scene.imports);
  const frames = visible(scene.frames);
  const primary = scene.frames.find((frame) => frame.primary) ?? scene.frames[0];
  const edges = primary
    ? [...scene.imports, ...scene.frames.filter((frame) => !frame.primary)]
        .map((item) => {
          const itemCenter = center(item);
          return edgeBetween(pointOnFrame(primary, itemCenter), itemCenter);
        })
        .filter((edge) => intersects(view, edgeBounds(edge)))
    : [];
  const primarySymbols = new Map(
    scene.symbols
      .filter((symbol) => symbol.group === primary?.file_name)
      .map((symbol) => [symbol.name, symbol] as const),
  );
  const calls = primary
    ? scene.calls
        .map(({ from, to }) => {
          const source = primarySymbols.get(from);
          const target = primarySymbols.get(to);
          return source && target ? edgeBetween(center(source), center(target)) : null;
        })
        .filter((edge): edge is SceneEdge => edge !== null && intersects(view, edgeBounds(edge)))
    : [];
  const frameScale = clamp(1 / zoom, 1, 2);
  const tagScale = clamp(1 / zoom, 1, 1.6);
  const candidates: SceneLabel[] = [
    ...frames.map((frame) => ({
      key: `fl${frame.file_name}`,
      x: frame.x,
      y: frame.y,
      text: frame.file_name,
      frame: true,
      big: false,
      scale: frameScale,
    })),
    ...(tier === 0
      ? [
          ...symbols
            .filter((symbol) => symbol.height * zoom > 18)
            .map((symbol, index) => ({
              key: `ts${index}${symbol.name}`,
              x: symbol.x,
              y: symbol.y,
              text: symbol.name,
              frame: false,
              big: true,
              scale: tagScale,
            })),
          ...imports
            .filter((item) => item.height * zoom > 14)
            .map((item, index) => ({
              key: `ti${index}${item.module}`,
              x: item.x,
              y: item.y,
              text: item.module,
              frame: false,
              big: false,
              scale: tagScale,
            })),
        ]
      : []),
  ];
  return { view, tier, symbols, imports, frames, edges, calls, labels: uniqueLabels(candidates, 42 / zoom) };
}

export function visibleCodeRows(symbol: SceneSymbol, view: Rect, lineHeight: number): [number, number] | null {
  const first = Math.max(0, Math.floor((view.y - symbol.code_top) / lineHeight));
  const last = Math.min(symbol.lines_shown, Math.ceil((view.y + view.height - symbol.code_top) / lineHeight) + 1);
  return last > first ? [first, last] : null;
}

export function sourceOffset(source: string[], line: number): number {
  return source.slice(0, line - 1).reduce((offset, value) => offset + value.length + 1, 0);
}

export function skeletonMetrics(text: string, charWidth: number, symbolWidth: number) {
  const indent = (text.match(/^\s*/) ?? [''])[0].length;
  const characters = clamp(text.trimEnd().length, 2, 120);
  const contentWidth = Math.max(1, symbolWidth - 74);
  return {
    width: `${Math.min(98, (characters * charWidth * 100) / contentWidth)}%`,
    marginLeft: `${(Math.min(indent, 16) * charWidth * 100) / contentWidth}%`,
  };
}

export function fitScene(canvas: Size, viewport: Size) {
  const zoom = clampZoom(
    Math.min(viewport.width / (canvas.width + 120), viewport.height / (canvas.height + 120)),
    0.15,
    1,
  );
  return {
    zoom,
    pan: {
      x: viewport.width / 2 - (canvas.width / 2) * zoom,
      y: viewport.height / 2 - (canvas.height / 2) * zoom,
    },
  };
}

export function zoomSceneAt(currentZoom: number, currentPan: Point, anchor: Point, wheelDelta: number) {
  const factor = clamp(Math.exp(-wheelDelta * 0.01), 0.8, 1.25);
  const zoom = clampZoom(currentZoom * factor, 0.15, 4);
  return {
    zoom,
    pan: {
      x: anchor.x - (anchor.x - currentPan.x) * (zoom / currentZoom),
      y: anchor.y - (anchor.y - currentPan.y) * (zoom / currentZoom),
    },
  };
}

export function clientToScene(client: Point, bounds: Pick<Rect, 'x' | 'y'>, pan: Point, zoom: number): Point {
  return { x: (client.x - bounds.x - pan.x) / zoom, y: (client.y - bounds.y - pan.y) / zoom };
}

export function sceneSelection(start: Point, end: Point, moved: boolean, zoom: number): SceneSelection {
  const rect = rectFromPoints(start, end);
  return !moved || (rect.width * zoom < 4 && rect.height * zoom < 4)
    ? { kind: 'point', ...(moved ? start : end) }
    : { kind: 'rect', ...rect };
}
