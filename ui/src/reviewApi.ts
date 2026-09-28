import type { Annotation, ReviewData } from './types.ts';

function withToken(path: string, token: string | null): string {
  return `${path}${token ? `?token=${encodeURIComponent(token)}` : ''}`;
}

async function expectOk(response: Response): Promise<Response> {
  if (!response.ok) throw new Error(await response.text());
  return response;
}

export async function fetchReview(reviewId: string, token: string | null): Promise<ReviewData> {
  const response = await expectOk(await fetch(withToken(`/api/reviews/${encodeURIComponent(reviewId)}`, token)));
  return response.json() as Promise<ReviewData>;
}

export async function validateReviewToken(reviewId: string, token: string): Promise<void> {
  await expectOk(
    await fetch(`/api/reviews/${encodeURIComponent(reviewId)}/session`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ token }),
    }),
  );
}

export async function submitReview(reviewId: string, token: string | null, annotations: Annotation[]): Promise<void> {
  await expectOk(
    await fetch(withToken(`/api/reviews/${encodeURIComponent(reviewId)}/submit`, token), {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ annotations, generation: { variants_enabled: false, count: 1, guides: [] } }),
    }),
  );
}

export async function cancelReview(reviewId: string): Promise<void> {
  await expectOk(await fetch(`/api/reviews/${encodeURIComponent(reviewId)}/cancel`, { method: 'POST' }));
}

export async function replyToQuestion(
  reviewId: string,
  messageId: string,
  token: string | null,
  text: string,
): Promise<void> {
  await expectOk(
    await fetch(
      withToken(`/api/reviews/${encodeURIComponent(reviewId)}/messages/${encodeURIComponent(messageId)}/reply`, token),
      {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ text }),
      },
    ),
  );
}

export async function handoffQuestion(reviewId: string, messageId: string, token: string | null): Promise<void> {
  await expectOk(
    await fetch(
      withToken(`/api/reviews/${encodeURIComponent(reviewId)}/messages/${encodeURIComponent(messageId)}/chat`, token),
      { method: 'POST' },
    ),
  );
}
