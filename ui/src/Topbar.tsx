import { useCallback, useEffect, useRef, useState } from 'react';
import { clampZoom } from './geometry.ts';
import { localeNames, messages } from './i18n.ts';
import { useReviewStore } from './reviewStore.ts';
import type { Locale } from './types.ts';
import BrandMark from './BrandMark.tsx';

type Theme = 'dark' | 'light';

const themeMediaQuery = '(prefers-color-scheme: dark)';

function storedTheme(): Theme | null {
  try {
    const stored = localStorage.getItem('retake.theme');
    return stored === 'light' || stored === 'dark' ? stored : null;
  } catch {
    return null;
  }
}

function systemTheme(): Theme {
  return window.matchMedia?.(themeMediaQuery).matches ? 'dark' : 'light';
}

interface Props {
  editable: boolean;
  onBeforeZoom: () => void;
  onSubmit: () => void;
  onClose: () => void;
}

export default function Topbar({ editable, onBeforeZoom, onSubmit, onClose }: Props) {
  const locale = useReviewStore((state) => state.locale);
  const setLocale = useReviewStore((state) => state.setLocale);
  const review = useReviewStore((state) => state.review);
  const status = useReviewStore((state) => state.status);
  const zoom = useReviewStore((state) => state.zoom);
  const setZoom = useReviewStore((state) => state.setZoom);
  const annotationCount = useReviewStore((state) => state.annotations.length);
  const [languageOpen, setLanguageOpen] = useState(false);
  const themePreference = useRef<Theme | null>(storedTheme());
  const [theme, setTheme] = useState<Theme>(() => themePreference.current ?? systemTheme());
  const languageRef = useRef<HTMLDivElement | null>(null);
  const t = messages[locale];

  useEffect(() => {
    document.documentElement.lang = locale;
    document.title = t.appTitle;
    localStorage.setItem('retake.locale', locale);
  }, [locale, t.appTitle]);

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    document.documentElement.style.colorScheme = theme;
    const themeColor = document.querySelector<HTMLMetaElement>('meta[name="theme-color"]');
    if (themeColor) themeColor.content = theme === 'light' ? '#F4F4F6' : '#191A20';
  }, [theme]);

  useEffect(() => {
    if (themePreference.current) return;
    const media = window.matchMedia(themeMediaQuery);
    const followSystemTheme = (event: MediaQueryListEvent) => {
      if (!themePreference.current) setTheme(event.matches ? 'dark' : 'light');
    };
    media.addEventListener('change', followSystemTheme);
    return () => media.removeEventListener('change', followSystemTheme);
  }, []);

  useEffect(() => {
    if (!languageOpen) return;
    const closeOnOutside = (event: globalThis.PointerEvent) => {
      if (!languageRef.current?.contains(event.target as Node)) setLanguageOpen(false);
    };
    const closeOnEscape = (event: globalThis.KeyboardEvent) => {
      if (event.key === 'Escape') setLanguageOpen(false);
    };
    document.addEventListener('pointerdown', closeOnOutside);
    document.addEventListener('keydown', closeOnEscape);
    return () => {
      document.removeEventListener('pointerdown', closeOnOutside);
      document.removeEventListener('keydown', closeOnEscape);
    };
  }, [languageOpen]);

  const changeZoom = useCallback(
    (value: number | ((current: number) => number)) => {
      onBeforeZoom();
      setZoom(value);
    },
    [onBeforeZoom, setZoom],
  );
  const zoomOut = useCallback(() => changeZoom((current) => clampZoom(current / 1.25)), [changeZoom]);
  const resetZoom = useCallback(() => changeZoom(1), [changeZoom]);
  const zoomIn = useCallback(() => changeZoom((current) => clampZoom(current * 1.25)), [changeZoom]);
  const toggleTheme = useCallback(
    () =>
      setTheme((current) => {
        const next = current === 'dark' ? 'light' : 'dark';
        themePreference.current = next;
        try {
          localStorage.setItem('retake.theme', next);
        } catch {
          /* storage may be disabled */
        }
        return next;
      }),
    [],
  );
  const toggleLanguage = useCallback(() => setLanguageOpen((current) => !current), []);
  const selectLanguage = useCallback(
    (option: Locale) => {
      setLocale(option);
      setLanguageOpen(false);
    },
    [setLocale],
  );

  const closeWindow = useCallback(() => {
    if (!review) return;
    void fetch(`/api/reviews/${encodeURIComponent(review.review_id)}/close`, { method: 'POST' }).then(
      () => window.close(),
      () => window.close(),
    );
  }, [review]);

  return (
    <header className="topbar">
      {(status === 'updating' || status === 'submitting') && <span className="topbar-loading" aria-hidden="true" />}
      <div className="brand" role="img" aria-label="Retake">
        <BrandMark />
      </div>
      <div className="zoom-controls topbar-zoom">
        <button onClick={zoomOut} aria-label={t.zoomOut}>
          −
        </button>
        <button className={`zoom-value${zoom !== 1 ? ' zoom-changed' : ''}`} onClick={resetZoom} title={t.fit}>
          {Math.round(zoom * 100)}%
        </button>
        <button onClick={zoomIn} aria-label={t.zoomIn}>
          ＋
        </button>
      </div>
      <div className="topbar-actions">
        <button
          className="theme-toggle"
          type="button"
          aria-label={theme === 'dark' ? t.switchToLightTheme : t.switchToDarkTheme}
          title={theme === 'dark' ? t.switchToLightTheme : t.switchToDarkTheme}
          aria-pressed={theme === 'light'}
          onClick={toggleTheme}
        >
          <span aria-hidden="true">{theme === 'dark' ? '☼' : '☾'}</span>
        </button>
        <div className="language-select" ref={languageRef}>
          <button
            className="language-button"
            onClick={toggleLanguage}
            aria-label={t.changeLanguage}
            aria-expanded={languageOpen}
            aria-haspopup="menu"
          >
            <span aria-hidden="true">◎</span> {localeNames[locale]} <span className="chevron" aria-hidden="true" />
          </button>
          {languageOpen && (
            <div className="language-menu" role="menu">
              {(['en', 'ja'] as Locale[]).map((option) => (
                <button
                  key={option}
                  role="menuitemradio"
                  aria-checked={locale === option}
                  onClick={() => selectLanguage(option)}
                >
                  {localeNames[option]}
                  <span>{locale === option ? '✓' : ''}</span>
                </button>
              ))}
            </div>
          )}
        </div>
        {status !== 'submitted' && status !== 'cancelled' && status !== 'updating' && (
          <>
            <button className="button button-quiet" onClick={onClose} disabled={status !== 'ready'}>
              {t.closeReview}
            </button>
            <button className="button button-primary" onClick={onSubmit} disabled={!editable || !annotationCount}>
              {t.submit}
            </button>
          </>
        )}
        {(status === 'submitted' || status === 'updating') && (
          <button className="button button-quiet close-review" onClick={closeWindow}>
            {t.closeWindow}
          </button>
        )}
      </div>
    </header>
  );
}
