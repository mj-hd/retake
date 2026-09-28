import assert from 'node:assert/strict';
import test from 'node:test';
import { clampZoom, percentageInBounds, pointInImage, rectFromPoints, selectionFromPoints } from './geometry.ts';

test('normalizes points into rectangles and selections', () => {
  assert.deepEqual(rectFromPoints({ x: 8, y: 9 }, { x: 2, y: 3 }), { x: 2, y: 3, width: 6, height: 6 });
  assert.deepEqual(selectionFromPoints({ x: 1, y: 2 }, { x: 3, y: 4 }), { kind: 'point', x: 1, y: 2 });
  assert.deepEqual(selectionFromPoints({ x: 8, y: 9 }, { x: 2, y: 3 }), {
    kind: 'rect',
    x: 2,
    y: 3,
    width: 6,
    height: 6,
  });
});

test('maps and clamps client coordinates', () => {
  assert.deepEqual(
    pointInImage({ x: 70, y: 80 }, { x: 20, y: 30, width: 100, height: 100 }, { width: 200, height: 300 }),
    { x: 100, y: 150 },
  );
  assert.equal(percentageInBounds(175, { x: 50, width: 100 }), 100);
  assert.equal(clampZoom(8), 4);
});
