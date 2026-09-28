import { useCallback, useEffect, useMemo, useRef } from 'react';
import type { MutableRefObject, PointerEvent } from 'react';
import CodeDiff from './CodeDiff.tsx';
import CodeScene from './CodeScene.tsx';
import type { SceneView } from './CodeScene.tsx';
import CommentEditor from './CommentEditor.tsx';
import ImageDiff from './ImageDiff.tsx';
import SnapshotCanvas from './SnapshotCanvas.tsx';
import { pointInImage, percentageInBounds, selectionFromPoints } from './geometry.ts';
import { derivePanePresentation } from './reviewModel.ts';
import type { SnapshotGroup } from './reviewModel.ts';
import { useReviewStore } from './reviewStore.ts';
import type { Annotation, Drag, ReviewData, Snapshot } from './types.ts';

interface Props {
  group: SnapshotGroup;
  groupIndex: number;
  comparison: NonNullable<ReviewData['revisions']>[number] | undefined;
  visibleAnnotations: Annotation[];
  showingDiff: boolean;
  isCapturing: boolean;
  editable: boolean;
  token: string;
  pinRefs: MutableRefObject<Record<number, HTMLButtonElement | null>>;
  sceneFocus: MutableRefObject<Record<string, (x: number, y: number) => void>>;
  sceneViews: MutableRefObject<Record<string, SceneView>>;
  assetSrc: (snapshot: Snapshot) => string;
  onActivateAnnotation: (index: number) => void;
  onImageLoad: () => void;
}

const SOURCE_ZOOM = 1.6;

export default function ActiveCanvasPane({
  group,
  groupIndex,
  comparison,
  visibleAnnotations,
  showingDiff,
  isCapturing,
  editable,
  token,
  pinRefs,
  sceneFocus,
  sceneViews,
  assetSrc,
  onActivateAnnotation,
  onImageLoad,
}: Props) {
  const locale = useReviewStore((state) => state.locale);
  const zoom = useReviewStore((state) => state.zoom);
  const setZoom = useReviewStore((state) => state.setZoom);
  const pane = useReviewStore((state) => state.paneSizes[group.key]);
  const draft = useReviewStore((state) => state.draft);
  const setDraft = useReviewStore((state) => state.setDraft);
  const comment = useReviewStore((state) => state.comment);
  const setComment = useReviewStore((state) => state.setComment);
  const active = useReviewStore((state) => state.active);
  const setActive = useReviewStore((state) => state.setActive);
  const drag = useReviewStore((state) => state.drag);
  const setDrag = useReviewStore((state) => state.setDrag);
  const setAnnotations = useReviewStore((state) => state.setAnnotations);
  const setSliderPosition = useReviewStore((state) => state.setSliderPosition);
  const imageRefs = useRef<Record<string, HTMLImageElement | null>>({});
  const textareaRef = useRef<HTMLTextAreaElement | null>(null);
  const dragRef = useRef<Drag | null>(null);
  const { snapshot, entries, selected, currentDrag, comparisonSnapshot, imageWidth } = useMemo(
    () =>
      derivePanePresentation({
        group,
        groupIndex,
        zoom,
        sourceZoom: SOURCE_ZOOM,
        annotations: visibleAnnotations,
        draft,
        drag,
        comparison,
        pane,
      }),
    [comparison, draft, drag, group, groupIndex, pane, visibleAnnotations, zoom],
  );
  const sceneEntries = useMemo(
    () =>
      entries.map(({ annotation, index }) => ({
        index,
        selection: annotation.selection,
        comment: annotation.comment,
      })),
    [entries],
  );

  useEffect(() => {
    if (selected) textareaRef.current?.focus({ preventScroll: true });
  }, [selected]);

  const imagePoint = useCallback(
    (snapshotId: string, clientX: number, clientY: number) => {
      const image = imageRefs.current[snapshotId];
      if (!image || snapshot.id !== snapshotId) return null;
      const bounds = image.getBoundingClientRect();
      return pointInImage(
        { x: clientX, y: clientY },
        { x: bounds.left, y: bounds.top, width: bounds.width, height: bounds.height },
        snapshot,
      );
    },
    [snapshot],
  );

  const onPointerDown = useCallback(
    (snapshotId: string, event: PointerEvent<HTMLDivElement>) => {
      if (!editable || event.button !== 0) return;
      const point = imagePoint(snapshotId, event.clientX, event.clientY);
      if (!point) return;
      event.preventDefault();
      event.currentTarget.setPointerCapture(event.pointerId);
      const next = {
        snapshotId,
        pointerId: event.pointerId,
        startX: point.x,
        startY: point.y,
        x: point.x,
        y: point.y,
      };
      dragRef.current = next;
      setDrag(next);
      setDraft(null);
      setActive(null);
    },
    [editable, imagePoint, setActive, setDraft, setDrag],
  );

  const onPointerMove = useCallback(
    (event: PointerEvent<HTMLDivElement>) => {
      const current = dragRef.current;
      if (!current || current.pointerId !== event.pointerId) return;
      const point = imagePoint(current.snapshotId, event.clientX, event.clientY);
      if (!point) return;
      const next = { ...current, x: point.x, y: point.y };
      dragRef.current = next;
      setDrag(next);
    },
    [imagePoint, setDrag],
  );

  const onPointerUp = useCallback(
    (event: PointerEvent<HTMLDivElement>) => {
      const current = dragRef.current;
      if (!current || current.pointerId !== event.pointerId) return;
      const end = imagePoint(current.snapshotId, event.clientX, event.clientY) ?? { x: current.x, y: current.y };
      const selection = selectionFromPoints({ x: current.startX, y: current.startY }, end);
      dragRef.current = null;
      setDrag(null);
      setDraft({ snapshotId: current.snapshotId, selection });
      setComment('');
    },
    [imagePoint, setComment, setDraft, setDrag],
  );

  const onPointerCancel = useCallback(() => {
    dragRef.current = null;
    setDrag(null);
  }, [setDrag]);

  const addAnnotation = useCallback(() => {
    if (!draft || !comment.trim()) return;
    setAnnotations((current) => [
      ...current,
      { snapshot_id: draft.snapshotId, selection: draft.selection, comment: comment.trim() },
    ]);
    setDraft(null);
    setComment('');
  }, [comment, draft, setAnnotations, setComment, setDraft]);

  const onSliderPointer = useCallback(
    (event: PointerEvent<HTMLDivElement>) => {
      const bounds = event.currentTarget.getBoundingClientRect();
      setSliderPosition(percentageInBounds(event.clientX, { x: bounds.left, width: bounds.width }));
    },
    [setSliderPosition],
  );

  const rememberSceneView = useCallback(
    (view: SceneView) => {
      sceneViews.current[group.key] = view;
    },
    [group.key, sceneViews],
  );
  const setSceneDraft = useCallback(
    (selection: Annotation['selection'] | null) => {
      if (editable) setDraft(selection ? { snapshotId: snapshot.id, selection } : null);
    },
    [editable, setDraft, snapshot.id],
  );
  const activateSceneAnnotation = useCallback(
    (index: number | null) => {
      if (index === null) setActive(null);
      else onActivateAnnotation(index);
    },
    [onActivateAnnotation, setActive],
  );
  const registerSceneFocus = useCallback(
    (focus: (x: number, y: number) => void) => {
      sceneFocus.current[snapshot.id] = focus;
    },
    [sceneFocus, snapshot.id],
  );

  return (
    <div
      className={`canvas-pane ${snapshot.layout === 'document' ? 'document-pane' : ''} ${snapshot.scene_url ? 'scene-pane' : ''} ${isCapturing ? 'capture-pane' : ''}`}
      data-snapshot-id={group.key}
      data-group-key={group.key}
      data-group-index={groupIndex}
    >
      {showingDiff && comparisonSnapshot ? (
        snapshot.scene_url && comparisonSnapshot.scene_url ? (
          <CodeDiff before={snapshot} after={comparisonSnapshot} token={token} locale={locale} />
        ) : (
          <ImageDiff
            before={snapshot}
            after={comparisonSnapshot}
            assetSrc={assetSrc}
            onImageLoad={onImageLoad}
            onSliderPointer={onSliderPointer}
          />
        )
      ) : snapshot.scene_url ? (
        <CodeScene
          sceneUrl={snapshot.scene_url}
          token={token}
          zoom={zoom}
          setZoom={setZoom}
          initialView={sceneViews.current[group.key]}
          onViewChange={rememberSceneView}
          entries={sceneEntries}
          active={active}
          draft={selected}
          draftEditor={selected ? <CommentEditor textareaRef={textareaRef} onSubmit={addAnnotation} /> : null}
          onDraft={setSceneDraft}
          onActivate={activateSceneAnnotation}
          editable={editable}
          registerFocus={registerSceneFocus}
        />
      ) : (
        <SnapshotCanvas
          snapshot={snapshot}
          imageWidth={imageWidth}
          entries={entries}
          selected={selected}
          currentDrag={currentDrag}
          imageRefs={imageRefs}
          pinRefs={pinRefs}
          textareaRef={textareaRef}
          assetSrc={assetSrc}
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={onPointerUp}
          onPointerCancel={onPointerCancel}
          onActivate={onActivateAnnotation}
          onAddAnnotation={addAnnotation}
          onImageLoad={onImageLoad}
        />
      )}
      <div className="canvas-caption">
        <span className="caption-dot" />
        {snapshot.label}
      </div>
    </div>
  );
}
