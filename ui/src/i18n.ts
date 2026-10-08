import type { Locale } from './types.ts';

export interface Messages {
  appTitle: string;
  review: string;
  feedback: string;
  submit: string;
  closeReview: string;
  add: string;
  placeholder: string;
  hint: string;
  empty: string;
  remove: string;
  close: string;
  status: Record<'loading' | 'updating' | 'ready' | 'submitting' | 'submitted', string>;
  keyboard: string;
  selection: string;
  pin: string;
  done: string;
  history: string;
  normal: string;
  compare: string;
  slider: string;
  difference: string;
  closeWindow: string;
  waiting: string;
  updating: string;
  latest: string;
  before: string;
  after: string;
  question: string;
  reply: string;
  replyPlaceholder: string;
  screens: string;
  diffControls: string;
  chat: string;
  missingReviewId: string;
  zoomOut: string;
  zoomIn: string;
  fit: string;
  changeLanguage: string;
  switchToLightTheme: string;
  switchToDarkTheme: string;
  codeDiff: string;
  codeDiffError: string;
  codeDiffLoading: string;
  noCodeChanges: string;
  codeSceneError: string;
  codeSceneLoading: string;
  moreRemoved: (count: number) => string;
  moreAdded: (count: number) => string;
  pinLabel: (number: number, comment: string) => string;
  removeLabel: (number: number) => string;
}

export const localeNames: Record<Locale, string> = { en: 'English', ja: '日本語' };

export const messages: Record<Locale, Messages> = {
  ja: {
    appTitle: 'retakeレビュー',
    review: 'レビュー',
    feedback: 'コメント',
    submit: '送信',
    closeReview: '閉じる',
    add: 'コメントを追加',
    placeholder: 'ここにコメントを入力…',
    hint: 'レビュー画面をクリックかドラッグで\nコメントを追加できます',
    empty: 'まだコメントはありません',
    remove: 'コメントを削除',
    pin: '位置',
    close: '閉じる',
    status: { loading: '読み込み中', updating: '更新中', ready: '編集中', submitting: '送信中', submitted: '送信済み' },
    keyboard: 'Enter で追加 · Shift + Enter で改行',
    selection: '選択中',
    done: 'レビューを送信しました',
    history: '変更履歴',
    normal: '通常',
    compare: '左右比較',
    slider: 'スライダー',
    difference: '差分',
    closeWindow: 'ウィンドウを閉じる',
    waiting: '送信中',
    updating: '反映中',
    latest: '最新版',
    before: '選択中',
    after: '最新版',
    question: '確認したいことがあります',
    reply: '返信する',
    replyPlaceholder: 'ここに返信を入力…',
    screens: '画面一覧',
    diffControls: '差分確認',
    chat: '元のチャット欄で回答する',
    missingReviewId: 'レビューIDがありません',
    zoomOut: '縮小',
    zoomIn: '拡大',
    fit: '全体表示',
    changeLanguage: '言語を変更',
    switchToLightTheme: 'ライトテーマに切り替え',
    switchToDarkTheme: 'ダークテーマに切り替え',
    codeDiff: 'コード差分',
    codeDiffError: '差分を読み込めませんでした',
    codeDiffLoading: '差分を読み込み中…',
    noCodeChanges: 'コードに変更はありません',
    codeSceneError: 'コード表示を読み込めませんでした',
    codeSceneLoading: 'コード表示を読み込み中…',
    moreRemoved: (count) => `ほか${count}行を削除`,
    moreAdded: (count) => `ほか${count}行を追加`,
    pinLabel: (number, comment) => `コメント ${number}: ${comment}`,
    removeLabel: (number) => `コメント ${number}を削除`,
  },
  en: {
    appTitle: 'retake review',
    review: 'Review',
    feedback: 'Comments',
    submit: 'Submit',
    closeReview: 'Close',
    add: 'Add comment',
    placeholder: 'Write a comment…',
    hint: 'Click or drag on the review\nto add a comment',
    empty: 'No comments yet',
    remove: 'Remove comment',
    pin: 'Pin',
    close: 'Close',
    status: {
      loading: 'Loading',
      updating: 'Updating',
      ready: 'Editing',
      submitting: 'Submitting',
      submitted: 'Submitted',
    },
    keyboard: 'Enter to add · Shift + Enter for a new line',
    selection: 'Selected',
    done: 'Review submitted',
    history: 'Change history',
    normal: 'Normal',
    compare: 'Side by side',
    slider: 'Slider',
    difference: 'Diff',
    closeWindow: 'Close window',
    waiting: 'Sending',
    updating: 'Updating…',
    latest: 'Latest',
    before: 'Selected',
    after: 'Latest',
    question: 'A question for you',
    reply: 'Send reply',
    replyPlaceholder: 'Write a reply…',
    screens: 'Screens',
    diffControls: 'Compare revisions',
    chat: 'Answer in the original chat',
    missingReviewId: 'Missing review ID',
    zoomOut: 'Zoom out',
    zoomIn: 'Zoom in',
    fit: 'Fit',
    changeLanguage: 'Change language',
    switchToLightTheme: 'Switch to light theme',
    switchToDarkTheme: 'Switch to dark theme',
    codeDiff: 'Code diff',
    codeDiffError: 'Could not load diff',
    codeDiffLoading: 'Loading diff…',
    noCodeChanges: 'No code changes',
    codeSceneError: 'Could not load code view',
    codeSceneLoading: 'Loading code view…',
    moreRemoved: (count) => `… ${count} more removed lines`,
    moreAdded: (count) => `… ${count} more added lines`,
    pinLabel: (number, comment) => `Comment ${number}: ${comment}`,
    removeLabel: (number) => `Remove comment ${number}`,
  },
};

export function initialLocale(): Locale {
  try {
    const stored = localStorage.getItem('retake.locale');
    if (stored === 'ja' || stored === 'en') return stored;
  } catch {
    // Storage can be unavailable on custom origins such as tauri://.
  }
  return navigator.language?.toLowerCase().startsWith('ja') ? 'ja' : 'en';
}

export function formatDateTime(value: string, locale: Locale): string {
  return new Date(value).toLocaleString(locale === 'ja' ? 'ja-JP' : 'en-US', {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  });
}

export function formatCount(count: number, locale: Locale): string {
  return locale === 'ja' ? `${count}件` : String(count);
}
