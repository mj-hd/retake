import { useCallback } from 'react';
import QuestionPanel from './QuestionPanel.tsx';
import { messages } from './i18n.ts';
import { handoffQuestion, replyToQuestion } from './reviewApi.ts';
import { findCurrentProgress, findOpenQuestion } from './reviewMessages.ts';
import { useReviewStore } from './reviewStore.ts';

interface Props {
  isCapturing: boolean;
  token: string | null;
}

export default function ReviewCommunication({ isCapturing, token }: Props) {
  const locale = useReviewStore((state) => state.locale);
  const review = useReviewStore((state) => state.review);
  const status = useReviewStore((state) => state.status);
  const replyText = useReviewStore((state) => state.replyText);
  const setReview = useReviewStore((state) => state.setReview);
  const setReplyText = useReviewStore((state) => state.setReplyText);
  const setReplying = useReviewStore((state) => state.setReplying);
  const setError = useReviewStore((state) => state.setError);
  const t = messages[locale];
  const openQuestion = findOpenQuestion(review?.messages);
  const latestRevision = review?.revisions?.[review.revisions.length - 1];
  const currentProgress = findCurrentProgress(review?.messages, latestRevision?.created_at);

  const sendReply = useCallback(async () => {
    if (!review || !openQuestion || openQuestion.handed_off_at || !replyText.trim()) return;
    setReplying(true);
    try {
      const reply = replyText.trim();
      await replyToQuestion(review.review_id, openQuestion.id, token, reply);
      setReview((current) =>
        current
          ? {
              ...current,
              messages: current.messages?.map((message) =>
                message.id === openQuestion.id ? { ...message, reply } : message,
              ),
            }
          : current,
      );
      setReplyText('');
    } catch (error) {
      setError(String(error));
    } finally {
      setReplying(false);
    }
  }, [openQuestion, replyText, review, setError, setReplyText, setReplying, setReview, token]);

  const answerInChat = useCallback(async () => {
    if (!review || !openQuestion || openQuestion.handed_off_at) return;
    setReplying(true);
    try {
      await handoffQuestion(review.review_id, openQuestion.id, token);
      setReview((current) =>
        current
          ? {
              ...current,
              messages: current.messages?.map((message) =>
                message.id === openQuestion.id ? { ...message, handed_off_at: new Date().toISOString() } : message,
              ),
            }
          : current,
      );
      setReplyText('');
    } catch (error) {
      setError(String(error));
    } finally {
      setReplying(false);
    }
  }, [openQuestion, review, setError, setReplyText, setReplying, setReview, token]);

  if (!isCapturing && !openQuestion) return null;
  return (
    <div className="canvas-center-overlay">
      {isCapturing && (
        <span className="capture-status" role="status">
          {status === 'submitting' ? t.status.submitting : (currentProgress?.text ?? t.waiting)}
        </span>
      )}
      {openQuestion && <QuestionPanel question={openQuestion} onReply={sendReply} onChat={answerInChat} />}
    </div>
  );
}
