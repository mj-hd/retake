export interface Snapshot {
  id: string;
  label: string;
  width: number;
  height: number;
  asset_url: string;
  renderer?: string;
  layout?: 'document' | 'canvas';
  zoom_group?: number | null;
  zoom_level?: number | null;
  scene_url?: string | null;
  diff_modes?: ('split' | 'slider' | 'difference')[];
}

export interface Selection {
  kind: 'point' | 'rect';
  x: number;
  y: number;
  width?: number;
  height?: number;
}

export interface Annotation {
  snapshot_id: string;
  selection: Selection;
  comment: string;
}

export interface ReviewMessage {
  id: string;
  kind: 'progress' | 'question';
  text: string;
  reply: string | null;
  handed_off_at?: string | null;
  created_at: string;
}

export interface ReviewData {
  review_id: string;
  status: string;
  snapshots: Snapshot[];
  annotations?: Annotation[] | null;
  revisions?: { number: number; created_at: string; snapshots: Snapshot[] }[];
  feedback?: { number: number; annotations: Annotation[] }[];
  messages?: ReviewMessage[];
}

export type Locale = 'ja' | 'en';
export type DiffMode = 'off' | 'split' | 'difference' | 'slider';
export type SidebarTab = 'comments' | 'history';
export type Drag = { snapshotId: string; pointerId: number; startX: number; startY: number; x: number; y: number };
