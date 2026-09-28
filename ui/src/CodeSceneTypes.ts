export interface SceneSelection {
  kind: 'point' | 'rect';
  x: number;
  y: number;
  width?: number;
  height?: number;
}

export interface SceneSymbol {
  name: string;
  kind: string;
  group: string;
  line: number;
  end_line: number;
  x: number;
  y: number;
  width: number;
  height: number;
  code_x: number;
  code_top: number;
  lines_shown: number;
}

export interface SceneFrame {
  file_name: string;
  primary: boolean;
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface SceneImport {
  module: string;
  line: number;
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface SceneData {
  canvas: { width: number; height: number };
  frames: SceneFrame[];
  imports: SceneImport[];
  symbols: SceneSymbol[];
  calls: { from: string; to: string }[];
  sources: Record<string, string[]>;
  tokens?: Record<string, { kinds: string[]; spans: number[] }>;
  char_width: number;
  line_height: number;
  header_height: number;
}

export interface SceneEntry {
  index: number;
  selection: SceneSelection;
  comment: string;
}

export interface SceneView {
  pan: { x: number; y: number };
}

export function sceneSelectionPlacement(
  selection: SceneSelection,
  zoom: number,
  pan: { x: number; y: number },
  viewSize: { width: number; height: number },
) {
  const width = selection.kind === 'rect' ? selection.width || 0 : 0;
  const height = selection.kind === 'rect' ? selection.height || 0 : 0;
  const x = selection.x + width;
  const y = selection.y + height;
  const edgeRight = pan.x + x * zoom > viewSize.width * 0.65;
  const edgeBottom = pan.y + y * zoom > viewSize.height * 0.7;
  return {
    x,
    y,
    edgeRight,
    edgeBottom,
  };
}
