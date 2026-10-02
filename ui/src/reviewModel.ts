import type { Annotation, DiffMode, Drag, ReviewData, Selection, Snapshot } from './types.ts';

export interface SnapshotGroup {
  key: string;
  levels: Snapshot[];
}

type ReviewRevision = NonNullable<ReviewData['revisions']>[number];

interface ReviewPresentationOptions {
  review: ReviewData | null;
  selectedRevision: number;
  status: string;
  diffMode: DiffMode;
  annotations: Annotation[];
  capturedAnnotations: Annotation[];
}

export function snapshotGroupKey(snapshot: Snapshot): string {
  return snapshot.zoom_group == null ? snapshot.id : `g${snapshot.zoom_group}`;
}

export function groupSnapshots(snapshots: Snapshot[]): SnapshotGroup[] {
  const groups = new Map<string, Snapshot[]>();
  for (const snapshot of snapshots) {
    const key = snapshotGroupKey(snapshot);
    const levels = groups.get(key) ?? [];
    levels.push(snapshot);
    groups.set(key, levels);
  }
  return [...groups].map(([key, levels]) => ({
    key,
    levels: [...levels].sort((a, b) => (a.zoom_level ?? 0) - (b.zoom_level ?? 0)),
  }));
}

export function latestRevisionNumber(review: ReviewData | null): number {
  return review?.revisions?.[review.revisions.length - 1]?.number ?? 1;
}

export function isLatestRevisionComplete(review: ReviewData): boolean {
  const latest = review.revisions?.[review.revisions.length - 1];
  return review.status !== 'updating' && Boolean(latest?.snapshots.length);
}

export function activeGroupIndex(selectedGroup: number, groupCount: number): number {
  return Math.min(selectedGroup, Math.max(0, groupCount - 1));
}

export function supportedDiffModes(snapshot: Snapshot | undefined): Exclude<DiffMode, 'off'>[] {
  return snapshot?.diff_modes ?? (snapshot?.scene_url ? ['difference'] : ['split', 'slider', 'difference']);
}

export function filterAnnotationsForSnapshots(annotations: Annotation[], snapshots: Snapshot[]): Annotation[] {
  const snapshotIds = new Set(snapshots.map((snapshot) => snapshot.id));
  return annotations.filter((annotation) => snapshotIds.has(annotation.snapshot_id));
}

export function mergePendingRevision(
  fetched: ReviewData,
  pendingRevision: number | null,
  current: ReviewData | null,
): ReviewData {
  if (!pendingRevision || latestRevisionNumber(fetched) >= pendingRevision) return fetched;
  const optimistic = current?.revisions?.find((revision) => revision.number === pendingRevision);
  if (!optimistic) return fetched;
  return {
    ...fetched,
    status: 'updating',
    snapshots: [],
    revisions: [...(fetched.revisions ?? []), optimistic],
  };
}

export function snapshotAtZoom(group: SnapshotGroup, zoom: number, sourceZoom: number): Snapshot {
  return group.levels.length > 1 && zoom > sourceZoom ? group.levels[group.levels.length - 1] : group.levels[0];
}

export function matchingComparisonSnapshot(
  comparison: ReviewRevision | undefined,
  snapshot: Snapshot,
  groupIndex: number,
): Snapshot | undefined {
  return (
    comparison?.snapshots.find(
      (candidate) =>
        snapshot.zoom_group != null &&
        candidate.zoom_group === snapshot.zoom_group &&
        candidate.zoom_level === snapshot.zoom_level,
    ) ?? comparison?.snapshots[groupIndex]
  );
}

export function imageDisplayWidth(snapshot: Snapshot, pane: { width: number; height: number } | undefined): number {
  const effectiveWidth = Math.max(240, (pane?.width ?? 0) - 40);
  const effectiveHeight = Math.max(240, (pane?.height ?? 0) - 74);
  return Math.max(
    100,
    snapshot.layout === 'document'
      ? Math.min(effectiveWidth, snapshot.width * 2)
      : Math.min(effectiveWidth, (effectiveHeight * snapshot.width) / Math.max(1, snapshot.height)),
  );
}

interface PanePresentationOptions {
  group: SnapshotGroup;
  groupIndex: number;
  zoom: number;
  sourceZoom: number;
  annotations: Annotation[];
  draft: { snapshotId: string; selection: Selection } | null;
  drag: Drag | null;
  comparison: ReviewRevision | undefined;
  pane: { width: number; height: number } | undefined;
}

export function derivePanePresentation({
  group,
  groupIndex,
  zoom,
  sourceZoom,
  annotations,
  draft,
  drag,
  comparison,
  pane,
}: PanePresentationOptions) {
  const snapshot = snapshotAtZoom(group, zoom, sourceZoom);
  return {
    snapshot,
    entries: annotations
      .map((annotation, index) => ({ annotation, index }))
      .filter(({ annotation }) => annotation.snapshot_id === snapshot.id),
    selected: draft?.snapshotId === snapshot.id ? draft.selection : null,
    currentDrag: drag?.snapshotId === snapshot.id ? drag : null,
    comparisonSnapshot: matchingComparisonSnapshot(comparison, snapshot, groupIndex),
    imageWidth: imageDisplayWidth(snapshot, pane),
  };
}

export function selectionCenter(selection: Selection): { x: number; y: number } {
  return {
    x: selection.x + (selection.kind === 'rect' ? (selection.width ?? 0) / 2 : 0),
    y: selection.y + (selection.kind === 'rect' ? (selection.height ?? 0) / 2 : 0),
  };
}

export function snapshotGroupIndex(snapshots: Snapshot[], target: Snapshot | undefined): number {
  if (!target) return -1;
  const targetKey = snapshotGroupKey(target);
  return [...new Set(snapshots.map(snapshotGroupKey))].indexOf(targetKey);
}

export function deriveReviewPresentation({
  review,
  selectedRevision,
  status,
  diffMode,
  annotations,
  capturedAnnotations,
}: ReviewPresentationOptions) {
  const displayedSnapshots =
    review?.revisions?.find((revision) => revision.number === selectedRevision)?.snapshots ?? review?.snapshots ?? [];
  const latestRevision = latestRevisionNumber(review);
  const isCapturing =
    status === 'submitting' ||
    (status === 'updating' && selectedRevision === latestRevision && displayedSnapshots.length === 0);
  const ghostSnapshots =
    status === 'submitting'
      ? displayedSnapshots
      : (review?.revisions?.find((revision) => revision.number === latestRevision - 1)?.snapshots ?? []);
  const comparison =
    selectedRevision < latestRevision
      ? review?.revisions?.find((revision) => revision.number === latestRevision)
      : undefined;
  const editable = status === 'ready' && selectedRevision === latestRevision && diffMode === 'off';
  const candidateAnnotations = isCapturing
    ? capturedAnnotations
    : editable
      ? annotations
      : (review?.feedback?.find((feedback) => feedback.number === selectedRevision)?.annotations ??
        (status === 'submitted' && selectedRevision === latestRevision ? annotations : []));
  const visibleAnnotations = filterAnnotationsForSnapshots(
    candidateAnnotations,
    isCapturing ? ghostSnapshots : displayedSnapshots,
  );

  return {
    displayedSnapshots,
    latestRevision,
    isCapturing,
    renderedSnapshots: isCapturing ? ghostSnapshots : displayedSnapshots,
    comparison,
    editable,
    visibleAnnotations,
    canCompare: selectedRevision < latestRevision && displayedSnapshots.length > 0 && !!comparison?.snapshots.length,
  };
}
