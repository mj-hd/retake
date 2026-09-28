import { useCallback } from 'react';
import { formatCount, formatDateTime, messages } from './i18n.ts';
import { useReviewStore } from './reviewStore.ts';
import type { Annotation, Snapshot } from './types.ts';

interface Props {
  visibleAnnotations: Annotation[];
  displayedSnapshots: Snapshot[];
  latestRevision: number;
  editable: boolean;
  onBeforeViewChange: () => void;
  onFocusAnnotation: (index: number, annotation: Annotation) => void;
}

function EmptyCommentsIcon() {
  return (
    <svg className="empty-icon" viewBox="0 0 96 96" aria-hidden="true">
      <circle className="empty-icon-background" cx="47" cy="47" r="43" />
      <path
        className="empty-icon-bubble"
        d="M64 47c0 9.1-7.4 16.5-16.5 16.5-3.2 0-6.2-.9-8.7-2.5l-8.1 2.3 2.4-7.9A16.5 16.5 0 1 1 64 47Z"
      />
      <circle className="empty-icon-add" cx="76" cy="75" r="14" />
      <path className="empty-icon-plus" d="M76 69v12M70 75h12" />
    </svg>
  );
}

export default function ReviewSidebar({
  visibleAnnotations,
  displayedSnapshots,
  latestRevision,
  editable,
  onBeforeViewChange,
  onFocusAnnotation,
}: Props) {
  const locale = useReviewStore((state) => state.locale);
  const review = useReviewStore((state) => state.review)!;
  const status = useReviewStore((state) => state.status);
  const sidebarTab = useReviewStore((state) => state.sidebarTab);
  const setSidebarTab = useReviewStore((state) => state.setSidebarTab);
  const selectedRevision = useReviewStore((state) => state.selectedRevision);
  const setSelectedRevision = useReviewStore((state) => state.setSelectedRevision);
  const setDiffMode = useReviewStore((state) => state.setDiffMode);
  const annotations = useReviewStore((state) => state.annotations);
  const setAnnotations = useReviewStore((state) => state.setAnnotations);
  const active = useReviewStore((state) => state.active);
  const setActive = useReviewStore((state) => state.setActive);
  const setDraft = useReviewStore((state) => state.setDraft);
  const t = messages[locale];
  const count = (value: number) => formatCount(value, locale);

  const showComments = useCallback(() => {
    onBeforeViewChange();
    setSidebarTab('comments');
    setDiffMode('off');
  }, [onBeforeViewChange, setDiffMode, setSidebarTab]);
  const showHistory = useCallback(() => {
    setSidebarTab('history');
    setDraft(null);
  }, [setDraft, setSidebarTab]);
  const showRevision = useCallback(
    (number: number) => {
      onBeforeViewChange();
      setSelectedRevision(number);
      setDiffMode('off');
      setActive(null);
      setDraft(null);
    },
    [onBeforeViewChange, setActive, setDiffMode, setDraft, setSelectedRevision],
  );
  const removeAnnotation = useCallback(
    (index: number) => {
      setAnnotations((current) => current.filter((_, item) => item !== index));
      setActive(null);
    },
    [setActive, setAnnotations],
  );

  return (
    <aside className="sidebar">
      <div className="sidebar-tabs" role="tablist" aria-label={t.review}>
        <button role="tab" aria-selected={sidebarTab === 'comments'} onClick={showComments}>
          {t.feedback} {count(visibleAnnotations.length)}
        </button>
        <button role="tab" aria-selected={sidebarTab === 'history'} onClick={showHistory}>
          {t.history} {count(review.revisions?.length ?? 0)}
        </button>
      </div>
      {sidebarTab === 'history' ? (
        <div className="history-list" role="tabpanel">
          {[...(review.revisions ?? [])].reverse().map((revision) => (
            <button
              key={revision.number}
              className={`history-row ${selectedRevision === revision.number ? 'history-row-active' : ''} ${revision.snapshots.length === 0 && status === 'updating' ? 'history-row-updating' : ''}`}
              onClick={() => showRevision(revision.number)}
            >
              <span>{formatDateTime(revision.created_at, locale)}</span>
              {revision.snapshots.length === 0 && status === 'updating' && (
                <span className="visually-hidden">{t.updating}</span>
              )}
              <span className="history-row-meta">
                {revision.number === latestRevision && !(revision.snapshots.length === 0 && status === 'updating') && (
                  <small>{t.latest}</small>
                )}
                <small>
                  {t.feedback}{' '}
                  {count(
                    revision.number === latestRevision && status === 'ready'
                      ? annotations.length
                      : (review.feedback?.find((feedback) => feedback.number === revision.number)?.annotations.length ??
                          0),
                  )}
                </small>
              </span>
            </button>
          ))}
        </div>
      ) : (
        <div className="comment-list" role="tabpanel">
          {visibleAnnotations.length ? (
            visibleAnnotations.map((annotation, index) => (
              <div className="comment-item" data-comment-index={index} key={index}>
                <button
                  className={`comment-row ${active === index ? 'comment-row-active' : ''}`}
                  onClick={() => onFocusAnnotation(index, annotation)}
                >
                  <span className="row-number">{index + 1}</span>
                  <span className="row-body">
                    <strong>{annotation.comment}</strong>
                    <small>
                      {displayedSnapshots.find((snapshot) => snapshot.id === annotation.snapshot_id)?.label} ·{' '}
                      {annotation.selection.kind === 'rect' ? t.selection : t.pin}
                    </small>
                  </span>
                </button>
                {editable && (
                  <button
                    className="comment-delete"
                    aria-label={t.removeLabel(index + 1)}
                    onClick={() => removeAnnotation(index)}
                  >
                    ×
                  </button>
                )}
              </div>
            ))
          ) : (
            <div className="empty-state">
              <EmptyCommentsIcon />
              <strong>{t.empty}</strong>
              {editable && <p>{t.hint}</p>}
            </div>
          )}
        </div>
      )}
    </aside>
  );
}
