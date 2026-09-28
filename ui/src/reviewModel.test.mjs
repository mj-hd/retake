import assert from 'node:assert/strict';
import test from 'node:test';
import {
  deriveReviewPresentation,
  filterAnnotationsForSnapshots,
  groupSnapshots,
  mergePendingRevision,
  selectionCenter,
  snapshotGroupIndex,
} from './reviewModel.ts';

const snapshot = (id, zoomGroup = null, zoomLevel = null) => ({
  id,
  label: id,
  width: 100,
  height: 100,
  asset_url: `/assets/${id}`,
  zoom_group: zoomGroup,
  zoom_level: zoomLevel,
});

test('groups snapshots by zoom group and sorts their levels', () => {
  const groups = groupSnapshots([snapshot('source', 0, 1), snapshot('overview', 0, 0), snapshot('standalone')]);
  assert.deepEqual(
    groups.map((group) => [group.key, group.levels.map((level) => level.id)]),
    [
      ['g0', ['overview', 'source']],
      ['standalone', ['standalone']],
    ],
  );
});

test('derives the capturing presentation from review state', () => {
  const previous = snapshot('previous');
  const review = {
    review_id: 'review',
    status: 'updating',
    snapshots: [],
    revisions: [
      { number: 1, created_at: '2026-01-01T00:00:00Z', snapshots: [previous] },
      { number: 2, created_at: '2026-01-01T00:01:00Z', snapshots: [] },
    ],
  };
  const capturedAnnotations = [{ snapshot_id: 'previous', selection: { kind: 'point', x: 1, y: 2 }, comment: 'x' }];
  const presentation = deriveReviewPresentation({
    review,
    selectedRevision: 2,
    status: 'updating',
    diffMode: 'off',
    annotations: [],
    capturedAnnotations,
  });
  assert.equal(presentation.isCapturing, true);
  assert.deepEqual(presentation.renderedSnapshots, [previous]);
  assert.deepEqual(presentation.visibleAnnotations, capturedAnnotations);
});

test('finds selection centers and snapshot group indices', () => {
  const levels = [snapshot('overview', 2, 0), snapshot('source', 2, 1), snapshot('other')];
  assert.deepEqual(selectionCenter({ kind: 'rect', x: 10, y: 20, width: 8, height: 6 }), { x: 14, y: 23 });
  assert.equal(snapshotGroupIndex(levels, levels[1]), 0);
  assert.equal(snapshotGroupIndex(levels, levels[2]), 1);
});

test('keeps an optimistic revision while the server is still updating', () => {
  const first = { number: 1, created_at: '2026-01-01T00:00:00Z', snapshots: [snapshot('first')] };
  const optimistic = { number: 2, created_at: '2026-01-01T00:01:00Z', snapshots: [] };
  const fetched = { review_id: 'review', status: 'submitted', snapshots: first.snapshots, revisions: [first] };
  const current = { ...fetched, status: 'updating', snapshots: [], revisions: [first, optimistic] };
  assert.deepEqual(mergePendingRevision(fetched, 2, current), {
    ...fetched,
    status: 'updating',
    snapshots: [],
    revisions: [first, optimistic],
  });
});

test('removes annotations whose snapshots are from another revision', () => {
  const valid = { snapshot_id: 'current', selection: { kind: 'point', x: 1, y: 2 }, comment: 'valid' };
  const stale = { snapshot_id: 'previous', selection: { kind: 'point', x: 3, y: 4 }, comment: 'stale' };
  assert.deepEqual(filterAnnotationsForSnapshots([valid, stale], [snapshot('current')]), [valid]);
});
