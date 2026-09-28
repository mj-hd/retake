import { sceneSelectionPlacement } from './CodeSceneTypes.ts';
import type { SceneEntry, SceneSelection } from './CodeSceneTypes.ts';
import { messages } from './i18n.ts';
import { useReviewStore } from './reviewStore.ts';

interface Props {
  entries: SceneEntry[];
  active: number | null;
  draft: SceneSelection | null;
  rubber: SceneSelection | null;
  zoom: number;
  pan: { x: number; y: number };
  viewSize: { width: number; height: number };
  onActivate: (index: number | null) => void;
}

export default function CodeSceneAnnotations({
  entries,
  active,
  draft,
  rubber,
  zoom,
  pan,
  viewSize,
  onActivate,
}: Props) {
  const t = messages[useReviewStore((state) => state.locale)];
  const stopPointerPropagation = useCallback((event: PointerEvent) => event.stopPropagation(), []);
  const clearActive = useCallback(() => onActivate(null), [onActivate]);
  const toggleEntry = useCallback((index: number) => onActivate(active === index ? null : index), [active, onActivate]);
  return (
    <>
      {entries.map((entry) => {
        const placement = sceneSelectionPlacement(entry.selection, zoom, pan, viewSize);
        return (
          <div key={entry.index}>
            {entry.selection.kind === 'rect' && (
              <div
                className="scene-saved-box"
                style={{
                  left: entry.selection.x,
                  top: entry.selection.y,
                  width: entry.selection.width,
                  height: entry.selection.height,
                }}
              />
            )}
            <div
              className={`scene-pin-group ${active === entry.index ? 'scene-pin-group-active' : ''} ${placement.edgeRight ? 'scene-pin-edge-right' : ''} ${placement.edgeBottom ? 'scene-pin-edge-bottom' : ''}`}
              style={{ left: placement.x, top: placement.y, transform: `scale(${1 / zoom})` }}
              onPointerDown={stopPointerPropagation}
            >
              <button
                className={`scene-pin ${active === entry.index ? 'scene-pin-active' : ''}`}
                onClick={() => toggleEntry(entry.index)}
                aria-label={t.pinLabel(entry.index + 1, entry.comment)}
              >
                {entry.index + 1}
              </button>
              {active === entry.index && (
                <div className="scene-pin-popover">
                  <span className="popover-top">
                    <strong>#{entry.index + 1}</strong>
                    <button className="icon-button" onClick={clearActive} aria-label={t.close}>
                      ×
                    </button>
                  </span>
                  <p>{entry.comment}</p>
                </div>
              )}
            </div>
          </div>
        );
      })}

      {draft &&
        (() => {
          const placement = sceneSelectionPlacement(draft, zoom, pan, viewSize);
          return (
            <>
              {draft.kind === 'rect' && (
                <div
                  className="scene-draft-box"
                  style={{ left: draft.x, top: draft.y, width: draft.width, height: draft.height }}
                />
              )}
              <div
                className="scene-draft-group"
                style={{ left: placement.x, top: placement.y, transform: `scale(${1 / zoom})` }}
              >
                <span className="scene-draft-pin">+</span>
              </div>
            </>
          );
        })()}

      {rubber && (rubber.width ?? 0) > 0 && (
        <div
          className="scene-draft-box"
          style={{ left: rubber.x, top: rubber.y, width: rubber.width, height: rubber.height }}
        />
      )}
    </>
  );
}
import { useCallback } from 'react';
import type { PointerEvent } from 'react';
