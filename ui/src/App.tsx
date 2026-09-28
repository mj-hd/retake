import { useCallback, useEffect, useMemo, useRef } from 'react';
import ActiveCanvasPane from './ActiveCanvasPane.tsx';
import type { SceneView } from './CodeScene.tsx';
import Topbar from './Topbar.tsx';
import DiffTabs from './DiffTabs.tsx';
import ReviewCommunication from './ReviewCommunication.tsx';
import ReviewSidebar from './ReviewSidebar.tsx';
import ScreenNav from './ScreenNav.tsx';
import { formatDateTime, messages } from './i18n.ts';
import { useReviewStore } from './reviewStore.ts';
import { cancelReview, fetchReview, submitReview, validateReviewToken } from './reviewApi.ts';
import { useCanvasView } from './useCanvasView.ts';
import {
  activeGroupIndex,
  deriveReviewPresentation,
  filterAnnotationsForSnapshots,
  groupSnapshots,
  mergePendingRevision,
  selectionCenter,
  snapshotGroupIndex,
  supportedDiffModes,
} from './reviewModel.ts';
import { clampZoom } from './geometry.ts';
import type { Annotation, Snapshot } from './types.ts';

export default function App() {
  const locale = useReviewStore((state) => state.locale);
  const review = useReviewStore((state) => state.review);
  const selectedRevision = useReviewStore((state) => state.selectedRevision);
  const selectedGroup = useReviewStore((state) => state.selectedGroup);
  const diffMode = useReviewStore((state) => state.diffMode);
  const toastVisible = useReviewStore((state) => state.toastVisible);
  const annotations = useReviewStore((state) => state.annotations);
  const active = useReviewStore((state) => state.active);
  const status = useReviewStore((state) => state.status);
  const error = useReviewStore((state) => state.error);
  const setReview = useReviewStore((state) => state.setReview);
  const setSelectedRevision = useReviewStore((state) => state.setSelectedRevision);
  const setSelectedGroup = useReviewStore((state) => state.setSelectedGroup);
  const setDiffMode = useReviewStore((state) => state.setDiffMode);
  const setSidebarTab = useReviewStore((state) => state.setSidebarTab);
  const setToastVisible = useReviewStore((state) => state.setToastVisible);
  const setAnnotations = useReviewStore((state) => state.setAnnotations);
  const setDraft = useReviewStore((state) => state.setDraft);
  const setActive = useReviewStore((state) => state.setActive);
  const setZoom = useReviewStore((state) => state.setZoom);
  const setStatus = useReviewStore((state) => state.setStatus);
  const setError = useReviewStore((state) => state.setError);
  const t = messages[locale];
  const sceneViews = useRef<Record<string, SceneView>>({});
  const latestRevisionRef = useRef(1);
  const pendingRevisionRef = useRef<number | null>(null);
  const reviewLoadedRef = useRef(false);
  const captureAnnotationsRef = useRef<Annotation[]>([]);
  const pinRefs = useRef<Record<number, HTMLButtonElement | null>>({});
  const sceneFocus = useRef<Record<string, (x: number, y: number) => void>>({});
  const tokenRef = useRef(new URLSearchParams(location.hash.slice(1)).get('token'));

  const {
    displayedSnapshots,
    latestRevision,
    isCapturing,
    renderedSnapshots,
    comparison,
    editable,
    visibleAnnotations,
    canCompare,
  } = useMemo(
    () =>
      deriveReviewPresentation({
        review,
        selectedRevision,
        status,
        diffMode,
        annotations,
        capturedAnnotations: captureAnnotationsRef.current,
      }),
    [review, selectedRevision, status, diffMode, annotations],
  );
  const groups = useMemo(() => groupSnapshots(renderedSnapshots), [renderedSnapshots]);
  const navigationGroups = useMemo(() => groups.map((group) => group.levels), [groups]);
  const activeGroup = activeGroupIndex(selectedGroup, navigationGroups.length);
  const activeSnapshotGroup = groups[activeGroup];
  const activeSnapshot = activeSnapshotGroup?.levels[0];
  const availableDiffModes = supportedDiffModes(activeSnapshot);
  const showingDiff = canCompare && diffMode !== 'off' && availableDiffModes.includes(diffMode);
  const selectedRevisionData = review?.revisions?.find((revision) => revision.number === selectedRevision);
  const revisionLabel = selectedRevisionData ? formatDateTime(selectedRevisionData.created_at, locale) : '';
  const { canvasRef, gridRef, restoringView, rememberCanvasView, beginViewTransition, finishViewTransition } =
    useCanvasView({
      selectedRevision,
      activeGroup,
      isCapturing,
      groupsKey: groups.flatMap((group) => group.levels.map((snapshot) => snapshot.id)).join(':'),
    });

  useEffect(() => {
    const rid = location.pathname.split('/').filter(Boolean).pop();
    if (!rid) {
      setError(t.missingReviewId);
      return;
    }
    const token = tokenRef.current;
    const refresh = () =>
      fetchReview(rid, token)
        .then((data) => {
          const latest = data.revisions?.[data.revisions.length - 1]?.number ?? 1;
          const previousLatest = latestRevisionRef.current;
          const firstLoad = !reviewLoadedRef.current;
          const pendingRevision = pendingRevisionRef.current;
          const waitingForRevision = pendingRevision !== null && latest < pendingRevision;
          const revisionArrived = pendingRevision !== null && latest >= pendingRevision;
          const hasNewRevision = !firstLoad && latest > previousLatest;
          if (hasNewRevision && !revisionArrived) beginViewTransition();
          setSelectedRevision((current) => {
            if (waitingForRevision) return current;
            return firstLoad || current === previousLatest || current === pendingRevision ? latest : current;
          });
          if (hasNewRevision || revisionArrived) {
            setDiffMode('off');
            setSidebarTab('comments');
            setAnnotations([]);
            setDraft(null);
            setActive(null);
          }
          latestRevisionRef.current = latest;
          if (revisionArrived) pendingRevisionRef.current = null;
          reviewLoadedRef.current = true;
          if (firstLoad && data.annotations) {
            setAnnotations(data.annotations);
            captureAnnotationsRef.current = data.annotations;
          }
          setReview((current) => mergePendingRevision(data, pendingRevisionRef.current, current));
          setStatus(waitingForRevision ? 'updating' : data.status === 'pending' ? 'ready' : data.status);
          if (
            !waitingForRevision &&
            !firstLoad &&
            data.status === 'submitted' &&
            data.annotations &&
            latest === previousLatest
          )
            setAnnotations(data.annotations);
          if (token) {
            validateReviewToken(rid, token).then(
              () => history.replaceState(null, '', location.pathname),
              () => undefined,
            );
          }
        })
        .catch((err) => setError(String(err)));
    void refresh();
    // Keep questions and revisions live even while the review is still pending.
    const timer = window.setInterval(() => {
      if (document.visibilityState === 'visible') void refresh();
    }, 2500);
    return () => window.clearInterval(timer);
  }, []);

  useEffect(() => {
    if (!toastVisible) return;
    const timer = window.setTimeout(() => setToastVisible(false), 3000);
    return () => window.clearTimeout(timer);
  }, [toastVisible]);

  useEffect(() => {
    if (!editable) return;
    setAnnotations((current) => {
      const valid = filterAnnotationsForSnapshots(current, displayedSnapshots);
      return valid.length === current.length ? current : valid;
    });
  }, [displayedSnapshots, editable, setAnnotations]);

  const rememberOnScroll = useCallback(() => {
    if (!restoringView.current) rememberCanvasView();
  }, [rememberCanvasView, restoringView]);

  const submit = useCallback(async () => {
    if (!review) return;
    const validAnnotations = filterAnnotationsForSnapshots(annotations, displayedSnapshots);
    if (!validAnnotations.length) {
      if (annotations.length) setAnnotations(validAnnotations);
      return;
    }
    beginViewTransition();
    captureAnnotationsRef.current = validAnnotations;
    setStatus('submitting');
    try {
      await submitReview(review.review_id, tokenRef.current, validAnnotations);
      const nextRevision = latestRevisionRef.current + 1;
      pendingRevisionRef.current = nextRevision;
      setReview((prev) =>
        prev
          ? {
              ...prev,
              status: 'updating',
              snapshots: [],
              revisions: [
                ...(prev.revisions ?? []),
                { number: nextRevision, created_at: new Date().toISOString(), snapshots: [] },
              ],
            }
          : prev,
      );
      setSelectedRevision(nextRevision);
      setSidebarTab('history');
      setDiffMode('off');
      setAnnotations([]);
      setActive(null);
      setStatus('updating');
      setDraft(null);
      setToastVisible(true);
    } catch (err) {
      setError(String(err));
      setStatus('ready');
    }
  }, [
    annotations,
    beginViewTransition,
    displayedSnapshots,
    review,
    setActive,
    setAnnotations,
    setDiffMode,
    setDraft,
    setError,
    setReview,
    setSelectedRevision,
    setSidebarTab,
    setStatus,
    setToastVisible,
  ]);

  const closeReview = useCallback(async () => {
    if (!review) return;
    try {
      await cancelReview(review.review_id);
      setStatus('cancelled');
    } catch (err) {
      setError(String(err));
    }
  }, [review, setError, setStatus]);

  const assetSrc = useCallback(
    (snapshot: Snapshot) =>
      `${snapshot.asset_url}${tokenRef.current ? `?token=${encodeURIComponent(tokenRef.current)}` : ''}`,
    [],
  );

  const activateAnnotation = useCallback(
    (index: number) => {
      setSidebarTab('comments');
      setActive(active === index ? null : index);
      setDraft(null);
      requestAnimationFrame(() =>
        document
          .querySelector(`[data-comment-index="${index}"]`)
          ?.scrollIntoView({ behavior: 'smooth', block: 'nearest' }),
      );
    },
    [active, setActive, setDraft, setSidebarTab],
  );

  const focusAnnotation = useCallback(
    (index: number, annotation: Annotation) => {
      const target = displayedSnapshots.find((snapshot) => snapshot.id === annotation.snapshot_id);
      const { x, y } = selectionCenter(annotation.selection);
      const neededZoom = (target?.zoom_level ?? 0) > 0 ? 1.8 : 1;
      setZoom((current) => clampZoom(current < neededZoom ? neededZoom : current));
      const groupIndex = snapshotGroupIndex(displayedSnapshots, target);
      if (groupIndex >= 0) setSelectedGroup(groupIndex);
      setActive(index);
      setDraft(null);
      if (target?.scene_url) {
        window.setTimeout(() => sceneFocus.current[target.id]?.(x, y), 100);
      } else {
        requestAnimationFrame(() =>
          window.setTimeout(
            () => pinRefs.current[index]?.scrollIntoView({ behavior: 'smooth', block: 'center', inline: 'center' }),
            80,
          ),
        );
      }
    },
    [displayedSnapshots, setActive, setDraft, setSelectedGroup, setZoom],
  );

  return (
    <div className="app-shell">
      <Topbar editable={editable} onBeforeZoom={beginViewTransition} onSubmit={submit} onClose={closeReview} />

      {error && (
        <div className="error-banner" role="alert">
          {error}
        </div>
      )}
      {!review ? (
        <main className="loading-state">{t.status.loading}…</main>
      ) : (
        <main className="workspace">
          <section className="canvas-area" ref={canvasRef} aria-label={t.review}>
            {revisionLabel && (
              <div className="revision-header">
                <span>{revisionLabel}</span>
                <span className="revision-badge">
                  {selectedRevision === latestRevision ? t.latest : `${selectedRevision} / ${latestRevision}`}
                </span>
              </div>
            )}
            <ScreenNav groups={navigationGroups} assetSrc={assetSrc} onBeforeChange={beginViewTransition} />
            <div className="canvas-grid" ref={gridRef} onScrollCapture={rememberOnScroll}>
              <ReviewCommunication isCapturing={isCapturing} token={tokenRef.current} />

              {activeSnapshotGroup && (
                <ActiveCanvasPane
                  key={activeSnapshotGroup.key}
                  group={activeSnapshotGroup}
                  groupIndex={activeGroup}
                  comparison={comparison}
                  visibleAnnotations={visibleAnnotations}
                  showingDiff={showingDiff}
                  isCapturing={isCapturing}
                  editable={editable}
                  token={tokenRef.current ?? ''}
                  pinRefs={pinRefs}
                  sceneFocus={sceneFocus}
                  sceneViews={sceneViews}
                  assetSrc={assetSrc}
                  onActivateAnnotation={activateAnnotation}
                  onImageLoad={finishViewTransition}
                />
              )}
              {isCapturing && (
                <div className="capture-surface-effects" aria-hidden="true">
                  <span className="capture-shimmer" />
                  <span className="capture-border" />
                </div>
              )}
            </div>
          </section>

          <ReviewSidebar
            visibleAnnotations={visibleAnnotations}
            displayedSnapshots={displayedSnapshots}
            latestRevision={latestRevision}
            editable={editable}
            onBeforeViewChange={beginViewTransition}
            onFocusAnnotation={focusAnnotation}
          />
        </main>
      )}
      {review && (
        <DiffTabs canCompare={canCompare} supportedModes={availableDiffModes} onBeforeChange={beginViewTransition} />
      )}
      {toastVisible && (
        <div className="toast" role="status">
          ✓ {t.done}
        </div>
      )}
    </div>
  );
}
