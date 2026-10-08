import { useCallback, useRef } from 'react';
import type { ChangeEvent, KeyboardEvent, PointerEvent, RefObject } from 'react';
import { shouldSubmitComment } from './commentEditorModel.ts';
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
  const composingRef = useRef(false);
  const compositionEndedAtRef = useRef(0);
  const onKeyDown = useCallback(
    (event: KeyboardEvent<HTMLTextAreaElement>) => {
      const nativeEvent = event.nativeEvent;
      if (
        shouldSubmitComment(
          {
            key: event.key,
            shiftKey: event.shiftKey,
            isComposing: nativeEvent.isComposing,
            keyCode: nativeEvent.keyCode,
          },
          composingRef.current,
          performance.now() - compositionEndedAtRef.current < 200,
        )
      ) {
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
  const startComposition = useCallback(() => {
    composingRef.current = true;
    compositionEndedAtRef.current = 0;
  }, []);
  const endComposition = useCallback(() => {
    composingRef.current = false;
    compositionEndedAtRef.current = performance.now();
  }, []);
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
        onCompositionStart={startComposition}
        onCompositionEnd={endComposition}
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
