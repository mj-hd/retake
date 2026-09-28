import { useCallback, useEffect, useLayoutEffect, useRef } from 'react';
import { useReviewStore } from './reviewStore.ts';

const clampZoom = (zoom: number) => Math.min(4, Math.max(0.25, zoom));

function viewSurface(pane: HTMLElement): HTMLElement {
  return pane.querySelector<HTMLElement>('.diff-slider-scroll, .image-diff-highlight, .diff-side, .code-diff') ?? pane;
}

function viewImage(pane: HTMLElement): HTMLImageElement | null {
  return pane.querySelector<HTMLImageElement>('.canvas-stage img, .diff-slider img, .diff-overlay img, .diff-side img');
}

interface Options {
  selectedRevision: number;
  activeGroup: number;
  isCapturing: boolean;
  groupsKey: string;
}

export function useCanvasView({ selectedRevision, activeGroup, isCapturing, groupsKey }: Options) {
  const diffMode = useReviewStore((state) => state.diffMode);
  const zoom = useReviewStore((state) => state.zoom);
  const setZoom = useReviewStore((state) => state.setZoom);
  const paneSizes = useReviewStore((state) => state.paneSizes);
  const setPaneSizes = useReviewStore((state) => state.setPaneSizes);
  const canvasRef = useRef<HTMLElement | null>(null);
  const gridRef = useRef<HTMLDivElement | null>(null);
  const restoringView = useRef(false);
  const viewAnchors = useRef<Record<string, { x: number; y: number; image: boolean }>>({});
  const viewTransitionEpoch = useRef(0);

  const rememberCanvasView = useCallback(() => {
    const pane = gridRef.current?.querySelector<HTMLElement>('.canvas-pane');
    if (!pane) return;
    const key = pane.dataset.groupKey;
    if (!key) return;
    const surface = viewSurface(pane);
    const image = viewImage(pane);
    if (image && image.getBoundingClientRect().width) {
      const frame = surface.getBoundingClientRect();
      const rect = image.getBoundingClientRect();
      viewAnchors.current[key] = {
        x: Math.min(1, Math.max(0, (frame.left + frame.width / 2 - rect.left) / rect.width)),
        y: Math.min(1, Math.max(0, (frame.top + frame.height / 2 - rect.top) / rect.height)),
        image: true,
      };
    } else if (surface.classList.contains('code-diff')) {
      viewAnchors.current[key] = {
        x: surface.scrollLeft / Math.max(1, surface.scrollWidth - surface.clientWidth),
        y: surface.scrollTop / Math.max(1, surface.scrollHeight - surface.clientHeight),
        image: false,
      };
    }
  }, []);

  const beginViewTransition = useCallback(() => {
    rememberCanvasView();
    restoringView.current = true;
    viewTransitionEpoch.current++;
  }, [rememberCanvasView]);

  const restoreCanvasView = useCallback((): boolean => {
    const pane = gridRef.current?.querySelector<HTMLElement>('.canvas-pane');
    if (!pane) return false;
    const key = pane.dataset.groupKey;
    const anchor = key ? viewAnchors.current[key] : undefined;
    if (!anchor) return true;
    const surface = viewSurface(pane);
    const image = viewImage(pane);
    // A newly mounted pane starts with a fallback size. Wait for its measured
    // size before restoring or the saved position would clamp to the top.
    if (anchor.image && !paneSizes[pane.dataset.snapshotId ?? '']) return false;
    if (anchor.image && image && image.getBoundingClientRect().height) {
      const frame = surface.getBoundingClientRect();
      const rect = image.getBoundingClientRect();
      surface.scrollLeft += rect.left + rect.width * anchor.x - frame.left - frame.width / 2;
      surface.scrollTop += rect.top + rect.height * anchor.y - frame.top - frame.height / 2;
      return true;
    }
    if (!anchor.image && surface.classList.contains('code-diff')) {
      surface.scrollLeft = anchor.x * Math.max(0, surface.scrollWidth - surface.clientWidth);
      surface.scrollTop = anchor.y * Math.max(0, surface.scrollHeight - surface.clientHeight);
      return true;
    }
    return !anchor.image && !!pane.querySelector('.scene-viewport');
  }, [paneSizes]);

  const finishViewTransition = useCallback(() => {
    if (!restoreCanvasView()) return;
    const epoch = viewTransitionEpoch.current;
    requestAnimationFrame(() => {
      if (epoch === viewTransitionEpoch.current) restoringView.current = false;
    });
  }, [restoreCanvasView]);

  useLayoutEffect(() => {
    if (!isCapturing) finishViewTransition();
  }, [selectedRevision, activeGroup, diffMode, isCapturing, zoom, paneSizes, finishViewTransition]);

  useEffect(() => {
    const grid = gridRef.current;
    if (!grid) return;
    const observer = new ResizeObserver((entries) => {
      setPaneSizes((previous) => {
        let next = previous;
        for (const entry of entries) {
          const id = (entry.target as HTMLElement).dataset.snapshotId;
          if (!id) continue;
          const width = entry.target.clientWidth;
          const height = entry.target.clientHeight;
          if (previous[id]?.width === width && previous[id]?.height === height) continue;
          if (next === previous) next = { ...previous };
          next[id] = { width, height };
        }
        return next;
      });
    });
    grid.querySelectorAll('.canvas-pane:not(.scene-pane)').forEach((pane) => observer.observe(pane));
    return () => observer.disconnect();
  }, [groupsKey, activeGroup, setPaneSizes]);

  useEffect(() => {
    const element = canvasRef.current;
    if (!element) return;
    const onWheel = (event: WheelEvent) => {
      if (!event.ctrlKey && !event.metaKey) return;
      event.preventDefault();
      beginViewTransition();
      setZoom((current) => clampZoom(current * (event.deltaY > 0 ? 0.9 : 1.1)));
    };
    element.addEventListener('wheel', onWheel, { passive: false });
    return () => element.removeEventListener('wheel', onWheel);
  }, [beginViewTransition, setZoom]);

  return { canvasRef, gridRef, restoringView, rememberCanvasView, beginViewTransition, finishViewTransition };
}
