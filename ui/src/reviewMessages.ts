import type { ReviewMessage } from './types.ts';

export function findOpenQuestion(messages: ReviewMessage[] | undefined): ReviewMessage | undefined {
  if (!messages) return undefined;
  for (let index = messages.length - 1; index >= 0; index--) {
    const message = messages[index];
    if (message.kind === 'question' && !message.reply && !message.handed_off_at) return message;
  }
  return undefined;
}

export function findCurrentProgress(
  messages: ReviewMessage[] | undefined,
  revisionCreatedAt: string | undefined,
): ReviewMessage | undefined {
  const progress = [...(messages ?? [])].reverse().find((message) => message.kind === 'progress');
  if (!progress) return undefined;
  return new Date(progress.created_at).getTime() >= new Date(revisionCreatedAt ?? 0).getTime() ? progress : undefined;
}
