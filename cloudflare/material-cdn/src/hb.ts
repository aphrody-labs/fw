// HarfBuzz hb-subset (harfbuzzjs 1.6.4, wasm, no imports) driven through its C API: subset by codepoints and glyph ids, pin or limit
// variation axes (instancing), keep chosen layout features. One instance per call, so a request never shares linear memory.
import hbSubsetWasm from "harfbuzzjs/dist/harfbuzz-subset.wasm";

interface HbExports {
  memory: WebAssembly.Memory;
  malloc(n: number): number;
  free(p: number): void;
  hb_blob_create(data: number, length: number, mode: number, userData: number, destroy: number): number;
  hb_blob_get_data(blob: number, lengthPtr: number): number;
  hb_face_create(blob: number, index: number): number;
  hb_face_reference_blob(face: number): number;
  hb_set_add(set: number, value: number): void;
  hb_set_invert(set: number): void;
  hb_subset_input_create_or_fail(): number;
  hb_subset_input_unicode_set(input: number): number;
  hb_subset_input_glyph_set(input: number): number;
  hb_subset_input_set(input: number, which: number): number;
  hb_subset_input_set_flags(input: number, flags: number): void;
  hb_subset_input_pin_axis_to_default(input: number, face: number, tag: number): number;
  hb_subset_input_pin_axis_location(input: number, face: number, tag: number, value: number): number;
  hb_subset_input_set_axis_range(input: number, face: number, tag: number, min: number, max: number, defaultPtr: number): number;
  hb_subset_or_fail(face: number, input: number): number;
}

/** hb_subset_flags_t of the pinned HarfBuzz. */
export const Flag = { NoHinting: 0x1, NotdefOutline: 0x40, NoLayoutClosure: 0x200, OptimizeIupDeltas: 0x400 } as const;
const SETS_LAYOUT_FEATURE_TAG = 6;
const MODE_WRITABLE = 2;

export const tag = (s: string): number => ((s.charCodeAt(0) << 24) | (s.charCodeAt(1) << 16) | (s.charCodeAt(2) << 8) | s.charCodeAt(3)) >>> 0;

export interface SubsetSpec {
  unicodes: readonly number[];
  /** Source glyph ids kept in addition to the codepoints (the FILL=1 glyphs of Material Symbols). */
  glyphs?: readonly number[];
  flags: number;
  /** Axes pinned to a value. */
  pin?: Readonly<Record<string, number>>;
  /** Axes pinned to their default. */
  pinDefault?: readonly string[];
  /** Axes limited to a range. */
  limit?: Readonly<Record<string, readonly [number, number]>>;
  /** Layout features kept besides HarfBuzz's defaults; `*` keeps every feature. */
  features?: readonly string[];
}

export class SubsetError extends Error {}

export function instantiate(): HbExports {
  return new WebAssembly.Instance(hbSubsetWasm, {}).exports as unknown as HbExports;
}

/** Subsets the SFNT held in the wasm memory at `fontPointer` (length `fontLength`), which the caller allocated with `x.malloc`. */
export function subsetAt(x: HbExports, fontPointer: number, fontLength: number, spec: SubsetSpec): Uint8Array {
  const blob = x.hb_blob_create(fontPointer, fontLength, MODE_WRITABLE, 0, 0);
  const face = x.hb_face_create(blob, 0);
  const input = x.hb_subset_input_create_or_fail();
  if (!input) throw new SubsetError("hb_subset_input_create_or_fail");
  const unicodes = x.hb_subset_input_unicode_set(input);
  for (const cp of spec.unicodes) x.hb_set_add(unicodes, cp);
  const glyphs = x.hb_subset_input_glyph_set(input);
  for (const gid of spec.glyphs ?? []) x.hb_set_add(glyphs, gid);
  x.hb_subset_input_set_flags(input, spec.flags);
  for (const [axis, value] of Object.entries(spec.pin ?? {})) if (!x.hb_subset_input_pin_axis_location(input, face, tag(axis), value)) throw new SubsetError(`cannot pin axis ${axis} to ${value}`);
  for (const axis of spec.pinDefault ?? []) if (!x.hb_subset_input_pin_axis_to_default(input, face, tag(axis))) throw new SubsetError(`cannot pin axis ${axis} to its default`);
  for (const [axis, [min, max]] of Object.entries(spec.limit ?? {})) if (!x.hb_subset_input_set_axis_range(input, face, tag(axis), min, max, 0)) throw new SubsetError(`cannot limit axis ${axis} to ${min}..${max}`);
  if (spec.features?.length) {
    const features = x.hb_subset_input_set(input, SETS_LAYOUT_FEATURE_TAG);
    if (spec.features.includes("*")) x.hb_set_invert(features);
    else for (const f of spec.features) x.hb_set_add(features, tag(f));
  }
  const result = x.hb_subset_or_fail(face, input);
  if (!result) throw new SubsetError("hb_subset_or_fail: the subset could not be built");
  const out = x.hb_face_reference_blob(result);
  const lengthPointer = x.malloc(4);
  const data = x.hb_blob_get_data(out, lengthPointer);
  const length = new Uint32Array(x.memory.buffer, lengthPointer, 1)[0]!;
  return new Uint8Array(x.memory.buffer, data, length).slice();
}

/** Reads a stream into freshly allocated wasm memory without an intermediate copy of the whole font. */
export async function loadInto(x: HbExports, body: ReadableStream<Uint8Array>, size: number): Promise<{ pointer: number; length: number }> {
  const pointer = x.malloc(size);
  if (!pointer) throw new SubsetError(`out of memory for a ${size} byte font`);
  let offset = 0;
  const reader = body.getReader();
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    if (offset + value.length > size) throw new SubsetError("font larger than its recorded size");
    new Uint8Array(x.memory.buffer, pointer + offset, value.length).set(value);
    offset += value.length;
  }
  if (offset !== size) throw new SubsetError(`font truncated: ${offset} of ${size} bytes`);
  return { pointer, length: size };
}
