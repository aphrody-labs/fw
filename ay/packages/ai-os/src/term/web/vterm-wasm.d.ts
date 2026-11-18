// Types of vterm-wasm.js: the aysys.wasm virtual terminal binding (panes live in WASM linear memory).
export const VTERM_GPU_STRIDE_U32: 5;

export interface VTermTheme {
  idx: number;
  slug: string;
  name: string;
  bgArgb: number;
  fgArgb: number;
  cursorBgArgb: number;
  cursorFgArgb: number;
  selBgArgb: number;
  selFgArgb: number;
  accentArgb: number;
  bg: string;
  fg: string;
  cursorBgAbgr: number;
  cursorFgAbgr: number;
  selBgAbgr: number;
  selFgAbgr: number;
  accentAbgr: number;
  isDark: boolean;
}

export interface VTermMeta {
  abiVersion: number;
  cols: number;
  rows: number;
  cursorX: number;
  cursorY: number;
  cursorVisible: boolean;
  cursorStyle: number;
  altScreen: boolean;
  bracketedPaste: boolean;
  appCursor: boolean;
  mouseMode: number;
  titleDirty: boolean;
  mouseFormat: number;
  themeIdx: number;
  scrollCount: number;
  feedBytes: number;
  blockCount: number;
  failedCount: number;
  lastExitCode: number;
  dirtyMask: boolean;
  title: string;
  scrollbackLen: number;
  viewOffset: number;
  replyPending: number;
  kittyFlags: number;
  syncOutput: boolean;
  focusEvents: boolean;
  cwdDirty: boolean;
  clipDirty: boolean;
  cwd: string;
}

export interface VTermBlock {
  id: number;
  startRow: number;
  cmdRow: number;
  outRow: number;
  endRow: number;
  exitCode: number;
  state: "empty" | "prompt" | "input" | "running" | "done";
  command: string;
  failed: boolean;
}

export interface VTermWasmPane {
  feed(bytesOrText: string | Uint8Array): void;
  resize(cols: number, rows: number): void;
  setTheme(slug: string): number;
  resetDirty(): void;
  meta(): VTermMeta;
  blocks(): VTermBlock[];
  rowText(row: number): string;
  text(): string;
  scrollLines(delta: number): number;
  setScroll(offset: number): number;
  drainReplies(): Uint8Array;
  getLink(slot: number): string;
  drainClipboard(): string;
  dispose(): void;
  packGpuCells(): Uint32Array;
}

export interface VTermWasm {
  abiVersion: number;
  stateBytes: number;
  listThemes(): VTermTheme[];
  createPane(cols?: number, rows?: number, themeSlug?: string): VTermWasmPane;
}

export function loadVTermWasm(wasmUrl?: string): Promise<VTermWasm>;
