import { useCallback } from 'react';
import type { ChangeEvent, KeyboardEvent, PointerEvent, RefObject } from 'react';
import { messages } from './i18n.ts';
import { useReviewStore } from './reviewStore.ts';

interface Props {
  textareaRef: RefObject<HTMLTextAreaElement>;
  onSubmit: () => void;
}

export default function CommentEditor({ textareaRef, onSubmit }: Props) {
  const locale = useReviewStore((state) => state.locale);
  const comment = useReviewStore((state) => state.comment);
  const setComment = useReviewStore((state) => state.setComment);
  const setDraft = useReviewStore((state) => state.setDraft);
  const t = messages[locale];
  const onKeyDown = useCallback(
    (event: KeyboardEvent<HTMLTextAreaElement>) => {
      if (event.key === 'Enter' && !event.shiftKey && !event.nativeEvent.isComposing) {
        event.preventDefault();
        onSubmit();
      }
      if (event.key === 'Escape') setDraft(null);
    },
    [onSubmit, setDraft],
  );
  const stopPointerPropagation = useCallback((event: PointerEvent) => event.stopPropagation(), []);
  const closeEditor = useCallback(() => setDraft(null), [setDraft]);
  const updateComment = useCallback(
    (event: ChangeEvent<HTMLTextAreaElement>) => setComment(event.target.value),
    [setComment],
  );
  return (
    <div className="comment-editor" onPointerDown={stopPointerPropagation}>
      <div className="editor-title">
        <span>{t.add}</span>
        <button className="icon-button" onClick={closeEditor} aria-label={t.close}>
          ×
        </button>
      </div>
      <textarea
        ref={textareaRef}
        value={comment}
        onChange={updateComment}
        onKeyDown={onKeyDown}
        placeholder={t.placeholder}
        rows={2}
      />
      <div className="editor-actions">
        <small>{t.keyboard}</small>
        <button className="button button-primary" onClick={onSubmit} disabled={!comment.trim()}>
          {t.add}
        </button>
      </div>
    </div>
  );
}
