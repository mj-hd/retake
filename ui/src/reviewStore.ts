import { create } from 'zustand';
import { initialLocale } from './i18n.ts';
import type { Annotation, DiffMode, Drag, Locale, ReviewData, Selection, SidebarTab } from './types.ts';

interface ReviewState {
  locale: Locale;
  review: ReviewData | null;
  selectedRevision: number;
  selectedGroup: number;
  diffMode: DiffMode;
  sliderPosition: number;
  sidebarTab: SidebarTab;
  toastVisible: boolean;
  replyText: string;
  replying: boolean;
  annotations: Annotation[];
  draft: { snapshotId: string; selection: Selection } | null;
  comment: string;
  active: number | null;
  zoom: number;
  paneSizes: Record<string, { width: number; height: number }>;
  drag: Drag | null;
  status: string;
  error: string;
  setLocale: (locale: Locale) => void;
  setReview: (review: ReviewData | null | ((current: ReviewData | null) => ReviewData | null)) => void;
  setSelectedRevision: (value: number | ((current: number) => number)) => void;
  setSelectedGroup: (value: number) => void;
  setDiffMode: (mode: DiffMode) => void;
  setSliderPosition: (value: number) => void;
  setSidebarTab: (tab: SidebarTab) => void;
  setToastVisible: (visible: boolean) => void;
  setReplyText: (value: string) => void;
  setReplying: (value: boolean) => void;
  setAnnotations: (value: Annotation[] | ((current: Annotation[]) => Annotation[])) => void;
  setDraft: (value: ReviewState['draft']) => void;
  setComment: (value: string) => void;
  setActive: (value: number | null) => void;
  setZoom: (value: number | ((current: number) => number)) => void;
  setPaneSizes: (
    value: ReviewState['paneSizes'] | ((current: ReviewState['paneSizes']) => ReviewState['paneSizes']),
  ) => void;
  setDrag: (value: Drag | null) => void;
  setStatus: (value: string) => void;
  setError: (value: string) => void;
}

const resolve = <T>(value: T | ((current: T) => T), current: T): T =>
  typeof value === 'function' ? (value as (current: T) => T)(current) : value;

export const useReviewStore = create<ReviewState>((set) => ({
  locale: initialLocale(),
  review: null,
  selectedRevision: 1,
  selectedGroup: 0,
  diffMode: 'off',
  sliderPosition: 50,
  sidebarTab: 'comments',
  toastVisible: false,
  replyText: '',
  replying: false,
  annotations: [],
  draft: null,
  comment: '',
  active: null,
  zoom: 1,
  paneSizes: {},
  drag: null,
  status: 'loading',
  error: '',
  setLocale: (locale) => set({ locale }),
  setReview: (value) => set((state) => ({ review: resolve(value, state.review) })),
  setSelectedRevision: (value) => set((state) => ({ selectedRevision: resolve(value, state.selectedRevision) })),
  setSelectedGroup: (selectedGroup) => set({ selectedGroup }),
  setDiffMode: (diffMode) => set({ diffMode }),
  setSliderPosition: (sliderPosition) => set({ sliderPosition }),
  setSidebarTab: (sidebarTab) => set({ sidebarTab }),
  setToastVisible: (toastVisible) => set({ toastVisible }),
  setReplyText: (replyText) => set({ replyText }),
  setReplying: (replying) => set({ replying }),
  setAnnotations: (value) => set((state) => ({ annotations: resolve(value, state.annotations) })),
  setDraft: (draft) => set({ draft }),
  setComment: (comment) => set({ comment }),
  setActive: (active) => set({ active }),
  setZoom: (value) => set((state) => ({ zoom: resolve(value, state.zoom) })),
  setPaneSizes: (value) => set((state) => ({ paneSizes: resolve(value, state.paneSizes) })),
  setDrag: (drag) => set({ drag }),
  setStatus: (status) => set({ status }),
  setError: (error) => set({ error }),
}));
