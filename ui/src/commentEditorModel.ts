export interface CommentKeyInput {
  key: string;
  shiftKey: boolean;
  isComposing: boolean;
  keyCode: number;
}

export function shouldSubmitComment(
  event: CommentKeyInput,
  compositionActive: boolean,
  recentlyEndedComposition: boolean,
): boolean {
  return (
    event.key === 'Enter' &&
    !event.shiftKey &&
    !compositionActive &&
    !recentlyEndedComposition &&
    !event.isComposing &&
    event.keyCode !== 229
  );
}
