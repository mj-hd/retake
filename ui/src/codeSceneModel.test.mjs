import assert from 'node:assert/strict';
import test from 'node:test';
import { clientToScene, fitScene, sceneSelection, sceneTier, sceneViewport, zoomSceneAt } from './codeSceneModel.ts';

test('derives scene viewport and level of detail', () => {
  assert.deepEqual(sceneViewport(2, { x: 10, y: 20 }, { width: 200, height: 100 }), {
    x: -5,
    y: -10,
    width: 100,
    height: 50,
  });
  assert.equal(sceneTier(0.2), 0);
  assert.equal(sceneTier(0.4), 1);
  assert.equal(sceneTier(1), 2);
});

test('fits, zooms and converts scene coordinates', () => {
  const fitted = fitScene({ width: 880, height: 380 }, { width: 500, height: 250 });
  assert.equal(fitted.zoom, 0.5);
  assert.deepEqual(fitted.pan, { x: 30, y: 30 });
  const zoomed = zoomSceneAt(1, { x: 0, y: 0 }, { x: 100, y: 50 }, -100);
  assert.equal(zoomed.zoom, 1.25);
  assert.deepEqual(zoomed.pan, { x: -25, y: -12.5 });
  assert.deepEqual(clientToScene({ x: 70, y: 80 }, { x: 10, y: 20 }, { x: 20, y: 10 }, 2), { x: 20, y: 25 });
});

test('creates point or rectangle scene selections', () => {
  assert.deepEqual(sceneSelection({ x: 1, y: 2 }, { x: 3, y: 4 }, false, 1), { kind: 'point', x: 3, y: 4 });
  assert.deepEqual(sceneSelection({ x: 8, y: 9 }, { x: 2, y: 3 }, true, 1), {
    kind: 'rect',
    x: 2,
    y: 3,
    width: 6,
    height: 6,
  });
});
