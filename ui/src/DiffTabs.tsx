import { useCallback } from 'react';
import { messages } from './i18n.ts';
import { useReviewStore } from './reviewStore.ts';
import type { DiffMode } from './types.ts';

interface Props {
  canCompare: boolean;
  supportedModes: Exclude<DiffMode, 'off'>[];
  onBeforeChange: () => void;
}

export default function DiffTabs({ canCompare, supportedModes, onBeforeChange }: Props) {
  const locale = useReviewStore((state) => state.locale);
  const diffMode = useReviewStore((state) => state.diffMode);
  const setDiffMode = useReviewStore((state) => state.setDiffMode);
  const setSliderPosition = useReviewStore((state) => state.setSliderPosition);
  const t = messages[locale];
  const select = useCallback(
    (mode: DiffMode) => {
      onBeforeChange();
      if (mode === 'slider') setSliderPosition(50);
      setDiffMode(mode);
    },
    [onBeforeChange, setDiffMode, setSliderPosition],
  );
  return (
    <nav
      className={`diff-tabs ${canCompare ? 'diff-tabs-visible' : ''}`}
      role="tablist"
      aria-label={t.diffControls}
      aria-hidden={!canCompare}
    >
      <button role="tab" aria-selected={diffMode === 'off'} onClick={() => select('off')}>
        {t.normal}
      </button>
      {supportedModes.includes('split') && (
        <button role="tab" aria-selected={diffMode === 'split'} onClick={() => select('split')}>
          {t.compare}
        </button>
      )}
      {supportedModes.includes('slider') && (
        <button role="tab" aria-selected={diffMode === 'slider'} onClick={() => select('slider')}>
          {t.slider}
        </button>
      )}
      {supportedModes.includes('difference') && (
        <button role="tab" aria-selected={diffMode === 'difference'} onClick={() => select('difference')}>
          {t.difference}
        </button>
      )}
    </nav>
  );
}
