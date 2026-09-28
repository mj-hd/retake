import { messages } from './i18n.ts';
import { useReviewStore } from './reviewStore.ts';
import type { ReviewMessage } from './types.ts';

interface Props {
  question: ReviewMessage;
  onReply: () => void;
  onChat: () => void;
}

export default function QuestionPanel({ question, onReply, onChat }: Props) {
  const locale = useReviewStore((state) => state.locale);
  const replyText = useReviewStore((state) => state.replyText);
  const setReplyText = useReviewStore((state) => state.setReplyText);
  const replying = useReviewStore((state) => state.replying);
  const t = messages[locale];
  const updateReply = useCallback(
    (event: ChangeEvent<HTMLTextAreaElement>) => setReplyText(event.target.value),
    [setReplyText],
  );
  return (
    <div className="review-communication question-panel" role="group" aria-label={t.question}>
      <strong>{t.question}</strong>
      <p>{question.text}</p>
      <textarea value={replyText} onChange={updateReply} placeholder={t.replyPlaceholder} rows={2} />
      <div className="question-actions">
        <button onClick={onReply} disabled={replying || !replyText.trim()}>
          {t.reply}
        </button>
        <button className="question-chat-button" onClick={onChat} disabled={replying}>
          {t.chat}
        </button>
      </div>
    </div>
  );
}
import { useCallback } from 'react';
import type { ChangeEvent } from 'react';
