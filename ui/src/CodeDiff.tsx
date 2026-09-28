import { useEffect, useMemo, useState } from 'react';
import { compareLines } from './codeDiffModel.ts';
import { messages } from './i18n.ts';
import type { Snapshot, Locale } from './types.ts';

interface SceneSources {
  sources: Record<string, string[]>;
}

export default function CodeDiff({
  before,
  after,
  token,
  locale,
}: {
  before: Snapshot;
  after: Snapshot;
  token: string;
  locale: Locale;
}) {
  const t = messages[locale];
  const [scenes, setScenes] = useState<[SceneSources, SceneSources] | null>(null);
  const [error, setError] = useState(false);
  useEffect(() => {
    let active = true;
    setScenes(null);
    setError(false);
    const url = (s: Snapshot) => `${s.scene_url}${token ? `?token=${encodeURIComponent(token)}` : ''}`;
    Promise.all(
      [fetch(url(before)), fetch(url(after))].map(async (req) => {
        const res = await req;
        if (!res.ok) throw new Error('scene fetch failed');
        return res.json() as Promise<SceneSources>;
      }),
    )
      .then((data) => {
        if (active) setScenes(data as [SceneSources, SceneSources]);
      })
      .catch(() => {
        if (active) setError(true);
      });
    return () => {
      active = false;
    };
  }, [before.scene_url, after.scene_url, token]);
  const files = useMemo(() => {
    if (!scenes) return [];
    return [...new Set([...Object.keys(scenes[0].sources), ...Object.keys(scenes[1].sources)])]
      .map((name) => ({ name, lines: compareLines(scenes[0].sources[name] ?? [], scenes[1].sources[name] ?? [], t) }))
      .filter((f) => f.lines.length);
  }, [scenes, t]);
  return (
    <div className="code-diff" aria-label={t.codeDiff}>
      {error ? (
        t.codeDiffError
      ) : !scenes ? (
        t.codeDiffLoading
      ) : files.length ? (
        files.map((file) => (
          <section key={file.name}>
            <h2>{file.name}</h2>
            <pre>
              {file.lines.map((line, index) => (
                <div key={index} className={`code-diff-${line.kind}`}>
                  {line.kind === 'added' ? '+' : line.kind === 'removed' ? '−' : ' '}
                  {line.text || ' '}
                </div>
              ))}
            </pre>
          </section>
        ))
      ) : (
        <p>{t.noCodeChanges}</p>
      )}
    </div>
  );
}
