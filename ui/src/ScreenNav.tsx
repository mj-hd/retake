import { useCallback } from 'react';
import { messages } from './i18n.ts';
import { activeGroupIndex } from './reviewModel.ts';
import { useReviewStore } from './reviewStore.ts';
import type { Snapshot } from './types.ts';

interface Props {
  groups: Snapshot[][];
  assetSrc: (snapshot: Snapshot) => string;
  onBeforeChange: () => void;
}

export default function ScreenNav({ groups, assetSrc, onBeforeChange }: Props) {
  const locale = useReviewStore((state) => state.locale);
  const selectedGroup = useReviewStore((state) => state.selectedGroup);
  const setSelectedGroup = useReviewStore((state) => state.setSelectedGroup);
  const setDiffMode = useReviewStore((state) => state.setDiffMode);
  const setDraft = useReviewStore((state) => state.setDraft);
  const setActive = useReviewStore((state) => state.setActive);
  const activeGroup = activeGroupIndex(selectedGroup, groups.length);
  const t = messages[locale];
  const selectGroup = useCallback(
    (index: number) => {
      onBeforeChange();
      setSelectedGroup(index);
      setDiffMode('off');
      setDraft(null);
      setActive(null);
    },
    [onBeforeChange, setActive, setDiffMode, setDraft, setSelectedGroup],
  );

  if (groups.length <= 1) return null;
  return (
    <nav className="screen-nav" aria-label={t.screens}>
      {groups.map((levels, index) => {
        const preview = levels.find((snapshot) => (snapshot.zoom_level ?? 0) === 0) ?? levels[0];
        return (
          <button
            key={index}
            type="button"
            className={`screen-nav-item ${activeGroup === index ? 'screen-nav-active' : ''}`}
            aria-current={activeGroup === index ? 'true' : undefined}
            onClick={() => selectGroup(index)}
          >
            <img src={assetSrc(preview)} alt="" loading="lazy" />
            <span>{preview.label}</span>
          </button>
        );
      })}
    </nav>
  );
}
