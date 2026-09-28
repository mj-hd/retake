import { useCallback } from 'react';
import type { PointerEvent } from 'react';
import { messages } from './i18n.ts';
import { useReviewStore } from './reviewStore.ts';
import type { Snapshot } from './types.ts';

interface Props {
  before: Snapshot;
  after: Snapshot;
  assetSrc: (snapshot: Snapshot) => string;
  onImageLoad: () => void;
  onSliderPointer: (event: PointerEvent<HTMLDivElement>) => void;
}

function syncScroll(source: HTMLDivElement) {
  const other = [...source.parentElement!.querySelectorAll<HTMLDivElement>('.diff-side')].find(
    (element) => element !== source,
  );
  if (!other) return;
  const y = source.scrollTop / Math.max(1, source.scrollHeight - source.clientHeight);
  const x = source.scrollLeft / Math.max(1, source.scrollWidth - source.clientWidth);
  other.scrollTop = y * Math.max(0, other.scrollHeight - other.clientHeight);
  other.scrollLeft = x * Math.max(0, other.scrollWidth - other.clientWidth);
}

export default function ImageDiff({ before, after, assetSrc, onImageLoad, onSliderPointer }: Props) {
  const locale = useReviewStore((state) => state.locale);
  const diffMode = useReviewStore((state) => state.diffMode);
  const sliderPosition = useReviewStore((state) => state.sliderPosition);
  const zoom = useReviewStore((state) => state.zoom);
  const t = messages[locale];
  const startSliderDrag = useCallback(
    (event: PointerEvent<HTMLDivElement>) => {
      event.currentTarget.setPointerCapture(event.pointerId);
      onSliderPointer(event);
    },
    [onSliderPointer],
  );
  const moveSlider = useCallback(
    (event: PointerEvent<HTMLDivElement>) => {
      if (event.currentTarget.hasPointerCapture(event.pointerId)) onSliderPointer(event);
    },
    [onSliderPointer],
  );

  return (
    <div className={`image-diff ${diffMode === 'difference' ? 'image-diff-highlight' : ''}`}>
      {diffMode === 'slider' ? (
        <div className="diff-slider-scroll">
          <div
            className="diff-slider"
            style={{ width: `${zoom * 100}%` }}
            onPointerDown={startSliderDrag}
            onPointerMove={moveSlider}
          >
            <img src={assetSrc(before)} alt={t.before} draggable={false} onLoad={onImageLoad} />
            <img
              className="diff-slider-after"
              src={assetSrc(after)}
              alt={t.after}
              draggable={false}
              style={{ clipPath: `inset(0 0 0 ${sliderPosition}%)` }}
            />
            <span className="diff-slider-divider" style={{ left: `${sliderPosition}%` }}>
              <span aria-hidden="true">↔</span>
            </span>
            <span className="diff-slider-label diff-slider-before">{t.before}</span>
            <span className="diff-slider-label diff-slider-after-label">{t.after}</span>
          </div>
        </div>
      ) : diffMode === 'difference' ? (
        <div className="diff-overlay" style={{ width: `${zoom * 100}%` }}>
          <img src={assetSrc(before)} alt={t.before} onLoad={onImageLoad} />
          <img src={assetSrc(after)} alt={t.after} />
        </div>
      ) : (
        <>
          <div className="diff-side" onScroll={(event) => syncScroll(event.currentTarget)}>
            <strong>{t.before}</strong>
            <div className="diff-image-wrap" style={{ width: `${zoom * 100}%` }}>
              <img src={assetSrc(before)} alt={t.before} onLoad={onImageLoad} />
            </div>
          </div>
          <div className="diff-side" onScroll={(event) => syncScroll(event.currentTarget)}>
            <strong>{t.after}</strong>
            <div className="diff-image-wrap" style={{ width: `${zoom * 100}%` }}>
              <img src={assetSrc(after)} alt={t.after} />
            </div>
          </div>
        </>
      )}
    </div>
  );
}
