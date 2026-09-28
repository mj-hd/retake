import { useCallback, useEffect, useRef, useState } from 'react';
import type { PointerEvent as ReactPointerEvent, ReactNode, WheelEvent as ReactWheelEvent } from 'react';
import { flushSync } from 'react-dom';
import CodeSceneAnnotations from './CodeSceneAnnotations.tsx';
import CodeSceneCanvas from './CodeSceneCanvas.tsx';
import { messages } from './i18n.ts';
import { useReviewStore } from './reviewStore.ts';
import { sceneSelectionPlacement } from './CodeSceneTypes.ts';
import { clientToScene, fitScene, sceneSelection, zoomSceneAt } from './codeSceneModel.ts';
import { movedBeyond } from './geometry.ts';
import type { SceneData, SceneEntry, SceneSelection, SceneView } from './CodeSceneTypes.ts';

export type { SceneEntry, SceneSelection, SceneView } from './CodeSceneTypes.ts';

interface Props {
  sceneUrl: string;
  token: string;
  zoom: number;
  setZoom: (fn: (z: number) => number) => void;
  initialView?: SceneView;
  onViewChange?: (view: SceneView) => void;
  entries: SceneEntry[];
  active: number | null;
  draft: SceneSelection | null;
  draftEditor?: ReactNode;
  onDraft: (selection: SceneSelection | null) => void;
  onActivate: (index: number | null) => void;
  editable: boolean;
  registerFocus: (focus: (x: number, y: number) => void) => void;
}

type Drag = {
  mode: 'select' | 'pan';
  sx: number;
  sy: number;
  start: { x: number; y: number };
  startPan: { x: number; y: number };
  moved: boolean;
};

/** Infinite, zoomable code field with level-of-detail rendering. */
export default function CodeScene({
  sceneUrl,
  token,
  zoom,
  setZoom,
  initialView,
  onViewChange,
  entries,
  active,
  draft,
  draftEditor,
  onDraft,
  onActivate,
  editable,
  registerFocus,
}: Props) {
  const t = messages[useReviewStore((state) => state.locale)];
  const [scene, setScene] = useState<SceneData | null>(null);
  const [loadFailed, setLoadFailed] = useState(false);
  const [pan, setPan] = useState(() => initialView?.pan ?? { x: 60, y: 30 });
  const [rubber, setRubber] = useState<SceneSelection | null>(null);
  const viewRef = useRef<HTMLDivElement | null>(null);
  const dragRef = useRef<Drag | null>(null);
  const zoomRef = useRef(zoom);
  const panRef = useRef(pan);
  const autoFitRef = useRef(!initialView);
  const fitZoomRef = useRef<number | null>(null);
  const previousSizeRef = useRef<{ width: number; height: number } | null>(null);
  const [viewSize, setViewSize] = useState({ width: 0, height: 0 });
  zoomRef.current = zoom;
  panRef.current = pan;

  useEffect(() => {
    if (fitZoomRef.current !== null && zoom !== fitZoomRef.current) autoFitRef.current = false;
  }, [zoom]);

  useEffect(() => {
    const url = `${sceneUrl}${token ? `?token=${encodeURIComponent(token)}` : ''}`;
    setLoadFailed(false);
    fetch(url)
      .then((r) => (r.ok ? r.json() : Promise.reject(new Error('scene fetch failed'))))
      .then((data: SceneData) => {
        autoFitRef.current = !initialView;
        previousSizeRef.current = null;
        setScene(data);
      })
      .catch(() => {
        setScene(null);
        setLoadFailed(true);
      });
  }, [sceneUrl, token]);

  useEffect(() => {
    if (scene && viewSize.width) onViewChange?.({ pan });
  }, [scene, viewSize.width, pan, zoom, onViewChange]);

  useEffect(() => {
    const el = viewRef.current;
    if (!el || !scene) return;
    const observer = new ResizeObserver(() => {
      const { width, height } = el.getBoundingClientRect();
      const previous = previousSizeRef.current;
      if (previous?.width === width && previous.height === height) return;
      previousSizeRef.current = { width, height };
      setViewSize({ width, height });
      if (autoFitRef.current) {
        const fitted = fitScene(scene.canvas, { width, height });
        fitZoomRef.current = fitted.zoom;
        zoomRef.current = fitted.zoom;
        panRef.current = fitted.pan;
        setZoom(() => fitted.zoom);
        setPan(fitted.pan);
      } else if (previous) {
        setPan((p) => ({ x: p.x + (width - previous.width) / 2, y: p.y + (height - previous.height) / 2 }));
      }
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, [scene, setZoom]);

  useEffect(() => {
    const el = viewRef.current;
    if (!el) return;
    // Native, non-passive: ctrl/cmd + wheel zooms without page zoom.
    const onWheel = (e: WheelEvent) => {
      e.stopPropagation();
      const rect = el.getBoundingClientRect();
      if (e.ctrlKey || e.metaKey) {
        e.preventDefault();
        autoFitRef.current = false;
        const next = zoomSceneAt(
          zoomRef.current,
          panRef.current,
          { x: e.clientX - rect.left, y: e.clientY - rect.top },
          e.deltaY,
        );
        zoomRef.current = next.zoom;
        panRef.current = next.pan;
        flushSync(() => {
          setPan(next.pan);
          setZoom(() => next.zoom);
        });
      } else {
        e.preventDefault();
        autoFitRef.current = false;
        setPan((p) => ({ x: p.x - e.deltaX, y: p.y - e.deltaY }));
      }
    };
    el.addEventListener('wheel', onWheel, { passive: false });
    return () => el.removeEventListener('wheel', onWheel);
  }, [setZoom]);

  const focusAt = useCallback(
    (x: number, y: number) => {
      const el = viewRef.current;
      if (!el) return;
      const rect = el.getBoundingClientRect();
      setPan({ x: rect.width / 2 - x * zoom, y: rect.height / 2 - y * zoom });
    },
    [zoom],
  );

  useEffect(() => registerFocus(focusAt), [focusAt, registerFocus]);

  const toLogical = useCallback(
    (clientX: number, clientY: number) => {
      const rect = viewRef.current!.getBoundingClientRect();
      return clientToScene({ x: clientX, y: clientY }, { x: rect.left, y: rect.top }, pan, zoom);
    },
    [pan, zoom],
  );

  const onPointerDown = useCallback(
    (e: ReactPointerEvent) => {
      if (!scene || e.button === 2) return;
      (e.currentTarget as Element).setPointerCapture(e.pointerId);
      const mode: Drag['mode'] = e.shiftKey || e.button === 1 || !editable ? 'pan' : 'select';
      dragRef.current = {
        mode,
        sx: e.clientX,
        sy: e.clientY,
        start: toLogical(e.clientX, e.clientY),
        startPan: pan,
        moved: false,
      };
      if (mode === 'select') {
        onDraft(null);
        setRubber(null);
      }
    },
    [editable, onDraft, pan, scene, toLogical],
  );

  const onPointerMove = useCallback(
    (e: ReactPointerEvent) => {
      const drag = dragRef.current;
      if (!drag) return;
      if (movedBeyond({ x: drag.sx, y: drag.sy }, { x: e.clientX, y: e.clientY })) drag.moved = true;
      if (drag.mode === 'pan') {
        autoFitRef.current = false;
        setPan({ x: drag.startPan.x + (e.clientX - drag.sx), y: drag.startPan.y + (e.clientY - drag.sy) });
      } else if (drag.moved) {
        setRubber(sceneSelection(drag.start, toLogical(e.clientX, e.clientY), true, zoom));
      }
    },
    [toLogical, zoom],
  );

  const onPointerUp = useCallback(
    (e: ReactPointerEvent) => {
      const drag = dragRef.current;
      dragRef.current = null;
      setRubber(null);
      if (!drag || drag.mode !== 'select') return;
      onDraft(sceneSelection(drag.start, toLogical(e.clientX, e.clientY), drag.moved, zoom));
    },
    [onDraft, toLogical, zoom],
  );

  const onWheelFallback = useCallback((event: ReactWheelEvent) => event.preventDefault(), []);
  const draftPlacement = draft ? sceneSelectionPlacement(draft, zoom, pan, viewSize) : null;
  const draftScreen = draftPlacement
    ? { x: pan.x + draftPlacement.x * zoom, y: pan.y + draftPlacement.y * zoom }
    : null;

  if (!scene) {
    return (
      <div className="scene-viewport scene-loading" ref={viewRef}>
        {loadFailed ? t.codeSceneError : t.codeSceneLoading}
      </div>
    );
  }

  return (
    <div
      className="scene-viewport"
      ref={viewRef}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onPointerCancel={onPointerUp}
      onWheel={onWheelFallback}
    >
      <div
        className="scene-world"
        style={{
          transform: `translate(${pan.x}px, ${pan.y}px) scale(${zoom})`,
          width: scene.canvas.width,
          height: scene.canvas.height,
        }}
      >
        <CodeSceneCanvas scene={scene} zoom={zoom} pan={pan} viewSize={viewSize} />
        <CodeSceneAnnotations
          entries={entries}
          active={active}
          draft={draft}
          rubber={rubber}
          zoom={zoom}
          pan={pan}
          viewSize={viewSize}
          onActivate={onActivate}
        />
      </div>
      {draftScreen && draftPlacement && draftEditor && (
        <div
          className={`scene-draft-editor-anchor ${draftPlacement.edgeRight ? 'edge-right' : ''} ${draftPlacement.edgeBottom ? 'edge-bottom' : ''}`}
          style={{ left: draftScreen.x, top: draftScreen.y }}
          onPointerDown={(event) => event.stopPropagation()}
        >
          {draftEditor}
        </div>
      )}
    </div>
  );
}
