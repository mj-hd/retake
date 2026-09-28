import { useCallback } from 'react';
import type { MutableRefObject, PointerEvent, RefObject } from 'react';
import CommentEditor from './CommentEditor.tsx';
import { percentageRect, rectFromPoints, selectionPlacement } from './geometry.ts';
import { messages } from './i18n.ts';
import { useReviewStore } from './reviewStore.ts';
import type { Annotation, Drag, Selection, Snapshot } from './types.ts';

interface Props {
  snapshot: Snapshot;
  imageWidth: number;
  entries: { annotation: Annotation; index: number }[];
  selected: Selection | null;
  currentDrag: Drag | null;
  imageRefs: MutableRefObject<Record<string, HTMLImageElement | null>>;
  pinRefs: MutableRefObject<Record<number, HTMLButtonElement | null>>;
  textareaRef: RefObject<HTMLTextAreaElement>;
  assetSrc: (snapshot: Snapshot) => string;
  onPointerDown: (snapshotId: string, event: PointerEvent<HTMLDivElement>) => void;
  onPointerMove: (event: PointerEvent<HTMLDivElement>) => void;
  onPointerUp: (event: PointerEvent<HTMLDivElement>) => void;
  onPointerCancel: () => void;
  onActivate: (index: number) => void;
  onAddAnnotation: () => void;
  onImageLoad: () => void;
}

export default function SnapshotCanvas({
  snapshot,
  imageWidth,
  entries,
  selected,
  currentDrag,
  imageRefs,
  pinRefs,
  textareaRef,
  assetSrc,
  onPointerDown,
  onPointerMove,
  onPointerUp,
  onPointerCancel,
  onActivate,
  onAddAnnotation,
  onImageLoad,
}: Props) {
  const locale = useReviewStore((state) => state.locale);
  const active = useReviewStore((state) => state.active);
  const setActive = useReviewStore((state) => state.setActive);
  const zoom = useReviewStore((state) => state.zoom);
  const t = messages[locale];
  const size = { width: snapshot.width, height: snapshot.height };
  const startSelection = useCallback(
    (event: PointerEvent<HTMLDivElement>) => onPointerDown(snapshot.id, event),
    [onPointerDown, snapshot.id],
  );
  const stopPointerPropagation = useCallback((event: PointerEvent) => event.stopPropagation(), []);
  const closeActive = useCallback(() => setActive(null), [setActive]);

  return (
    <div
      className="canvas-stage"
      style={{ zoom, width: imageWidth }}
      onPointerDown={startSelection}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onPointerCancel={onPointerCancel}
    >
      <img
        ref={(element) => {
          imageRefs.current[snapshot.id] = element;
        }}
        draggable={false}
        src={assetSrc(snapshot)}
        alt={snapshot.label}
        onLoad={onImageLoad}
      />
      <div className="stage-overlay">
        {entries.map(({ annotation, index }) => {
          const selection = annotation.selection;
          const placement = selectionPlacement(selection, size);
          return (
            <div key={index}>
              {selection.kind === 'rect' && (
                <div
                  className="selection-box saved-box"
                  style={percentageRect(
                    {
                      x: selection.x,
                      y: selection.y,
                      width: selection.width ?? 0,
                      height: selection.height ?? 0,
                    },
                    size,
                  )}
                />
              )}
              <div
                className={`pin-group ${placement.edgeRight ? 'edge-right' : ''} ${placement.edgeBottom ? 'edge-bottom' : ''}`}
                style={{
                  left: `${(placement.x / snapshot.width) * 100}%`,
                  top: `${(placement.y / snapshot.height) * 100}%`,
                }}
                onPointerDown={stopPointerPropagation}
              >
                <button
                  ref={(element) => {
                    pinRefs.current[index] = element;
                  }}
                  className={`pin ${active === index ? 'pin-active' : ''}`}
                  onClick={() => onActivate(index)}
                  aria-label={t.pinLabel(index + 1, annotation.comment)}
                >
                  {index + 1}
                </button>
                {active === index && (
                  <div className="pin-popover">
                    <span className="popover-top">
                      <span className="popover-index">#{index + 1}</span>
                      <button className="icon-button" onClick={closeActive} aria-label={t.close}>
                        ×
                      </button>
                    </span>
                    <p>{annotation.comment}</p>
                  </div>
                )}
              </div>
            </div>
          );
        })}
        {currentDrag && (
          <div
            className="selection-box drawing-box"
            style={percentageRect(
              rectFromPoints({ x: currentDrag.startX, y: currentDrag.startY }, { x: currentDrag.x, y: currentDrag.y }),
              size,
            )}
          />
        )}
        {selected &&
          (() => {
            const placement = selectionPlacement(selected, size);
            return (
              <>
                {selected.kind === 'rect' && (
                  <div
                    className="selection-box drawing-box"
                    style={percentageRect(
                      {
                        x: selected.x,
                        y: selected.y,
                        width: selected.width ?? 0,
                        height: selected.height ?? 0,
                      },
                      size,
                    )}
                  />
                )}
                <div
                  className={`draft-anchor ${placement.edgeRight ? 'edge-right' : ''} ${placement.edgeBottom ? 'edge-bottom' : ''}`}
                  style={{
                    left: `${(placement.x / snapshot.width) * 100}%`,
                    top: `${(placement.y / snapshot.height) * 100}%`,
                  }}
                >
                  <span className="draft-pin" aria-hidden="true" />
                  <CommentEditor textareaRef={textareaRef} onSubmit={onAddAnnotation} />
                </div>
              </>
            );
          })()}
      </div>
    </div>
  );
}
