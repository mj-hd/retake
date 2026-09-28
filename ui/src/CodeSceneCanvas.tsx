import { useMemo } from 'react';
import type { ReactNode } from 'react';
import { deriveSceneCanvas, skeletonMetrics, sourceOffset, visibleCodeRows } from './codeSceneModel.ts';
import type { SceneData } from './CodeSceneTypes.ts';

interface Props {
  scene: SceneData;
  zoom: number;
  pan: { x: number; y: number };
  viewSize: { width: number; height: number };
}

// Captured Tree-sitter spans only change color, preserving monospaced metrics
// so line/column annotation coordinates remain stable.
function highlightCode(source: string, base: number, tokens?: { kinds: string[]; spans: number[] }): ReactNode[] {
  if (!tokens?.spans.length) return [source];
  const parts: ReactNode[] = [];
  const end = base + source.length;
  let cursor = base;
  for (let index = 0; index < tokens.spans.length; index += 3) {
    const tokenStart = tokens.spans[index];
    const tokenEnd = tokens.spans[index + 1];
    if (tokenEnd <= base) continue;
    if (tokenStart >= end) break;
    const start = Math.max(base, tokenStart);
    const finish = Math.min(end, tokenEnd);
    if (start > cursor) parts.push(source.slice(cursor - base, start - base));
    if (finish > start) {
      const kind = tokens.kinds[tokens.spans[index + 2]] ?? 'variable';
      parts.push(
        <span key={`${start}-${finish}`} className={`code-token-${kind}`}>
          {source.slice(start - base, finish - base)}
        </span>,
      );
      cursor = Math.max(cursor, finish);
    }
  }
  if (cursor < end) parts.push(source.slice(cursor - base));
  return parts;
}

export default function CodeSceneCanvas({ scene, zoom, pan, viewSize }: Props) {
  const { view, tier, symbols, imports, frames, edges, calls, labels } = useMemo(
    () => deriveSceneCanvas(scene, zoom, pan, viewSize),
    [scene, zoom, pan, viewSize],
  );
  const lineH = scene.line_height;

  return (
    <>
      <svg className="scene-edges" width={scene.canvas.width} height={scene.canvas.height}>
        {edges.map(([x0, y0, x1, y1], index) => (
          <line key={`e${index}`} x1={x0} y1={y0} x2={x1} y2={y1} stroke="var(--scene-edge)" strokeWidth={1} />
        ))}
        {calls.map(([x0, y0, x1, y1], index) => (
          <path
            key={`c${index}`}
            d={`M ${x0} ${y0} Q ${(x0 + x1) / 2 + (y1 - y0) * 0.12} ${(y0 + y1) / 2 - (x1 - x0) * 0.12} ${x1} ${y1}`}
            fill="none"
            stroke="var(--scene-call)"
            strokeWidth={1.2}
            strokeDasharray="6 5"
          />
        ))}
      </svg>
      {frames.map((frame) => (
        <div
          key={frame.file_name}
          className={`scene-frame ${frame.primary ? 'scene-frame-primary' : ''}`}
          style={{ left: frame.x, top: frame.y, width: frame.width, height: frame.height }}
        />
      ))}
      {imports.map((item, index) => (
        <div
          key={`i${index}`}
          className="scene-dep"
          style={{ left: item.x, top: item.y, width: item.width, height: item.height }}
        >
          {tier > 0 && (
            <>
              <span className="scene-dep-name">{item.module}</span>
              <span className="scene-dep-line">L{item.line}</span>
            </>
          )}
        </div>
      ))}
      {symbols.map((symbol, index) => {
        const source = scene.sources[symbol.group] ?? [];
        const code = source.slice(symbol.line - 1, symbol.line - 1 + symbol.lines_shown).join('\n');
        const codeOffset = sourceOffset(source, symbol.line);
        const rows = tier === 2 ? visibleCodeRows(symbol, view, lineH) : null;
        return (
          <div
            key={`s${index}`}
            className="scene-box"
            style={{ left: symbol.x, top: symbol.y, width: symbol.width, height: symbol.height }}
          >
            {tier > 0 && (
              <>
                <div className="scene-box-head">
                  <span className="scene-box-kind">{symbol.kind}</span>
                  <span className="scene-box-name">{symbol.name}</span>
                  <span className="scene-box-lines">
                    L{symbol.line}–{symbol.end_line}
                  </span>
                </div>
                {tier === 1 && <div className="scene-box-hint">{source[symbol.line - 1]?.trim().slice(0, 120)}</div>}
              </>
            )}
            {tier < 2 && (
              <div className="scene-skeleton" style={{ top: symbol.code_top - symbol.y }}>
                {Array.from({ length: Math.min(symbol.lines_shown, 22) }, (_, row) => {
                  const text = source[symbol.line - 1 + row] ?? '';
                  return <span key={row} style={skeletonMetrics(text, scene.char_width, symbol.width)} />;
                })}
              </div>
            )}
            {rows && (
              <>
                <pre
                  className="scene-code-numbers"
                  style={{
                    top: symbol.code_top - symbol.y,
                    lineHeight: `${lineH}px`,
                    clipPath: `inset(${rows[0] * lineH}px 0 0 0)`,
                  }}
                >
                  {Array.from({ length: symbol.lines_shown }, (_, row) => symbol.line + row).join('\n')}
                </pre>
                <pre
                  className="scene-code"
                  style={{
                    left: symbol.code_x - symbol.x,
                    top: symbol.code_top - symbol.y,
                    lineHeight: `${lineH}px`,
                    clipPath: `inset(${rows[0] * lineH}px 0 0 0)`,
                  }}
                >
                  {highlightCode(code, codeOffset, scene.tokens?.[symbol.group])}
                </pre>
              </>
            )}
          </div>
        );
      })}
      {labels.map((label) => (
        <span
          key={label.key}
          className="scene-anchor"
          style={{ left: label.x, top: label.y, transform: `scale(${label.scale})` }}
        >
          <span className={label.frame ? 'scene-frame-label' : label.big ? 'scene-tag scene-tag-name' : 'scene-tag'}>
            {label.text}
          </span>
        </span>
      ))}
    </>
  );
}
