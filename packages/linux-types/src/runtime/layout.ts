// Struct and union codecs over ArrayBuffer/DataView, driven by the layout descriptors the
// generator emits. No bun:ffi here: the codecs run anywhere; ffi.ts holds the Linux-only helpers.
//
// Value representation (the same for every struct):
//   i8 u8 i16 u16 i32 u32 f32 f64  -> number
//   i64 u64 and pointers           -> bigint (pointers are plain addresses; never numbers)
//   bool                           -> boolean
//   char[N]                        -> string (UTF-8 up to the first NUL), other 1-byte arrays -> Uint8Array / Int8Array
//   other arrays                   -> arrays of the element representation
//   nested struct / union          -> object; anonymous members are keys `_anon0`, `_anon1`, ...
//   bitfields                      -> number, or bigint when the declared type is 64-bit
//   flexible array member          -> the elements that fit between its offset and the end of the buffer
// Byte order is little-endian (every supported arch).

export type Scalar = "i8" | "u8" | "char" | "i16" | "u16" | "i32" | "u32" | "i64" | "u64" | "f32" | "f64" | "ptr" | "bool";

export interface Bits {
  readonly k: "bits";
  readonly t: Scalar;
  // Absolute bit offset from the start of the record, and width.
  readonly bit: number;
  readonly n: number;
}

export interface Bytes {
  readonly k: "bytes";
  readonly n: number;
}

export interface Flex {
  readonly k: "flex";
  readonly of: FieldType;
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export type AnyLayout = Layout<any>;
export type ArrayType = readonly [of: FieldType, count: number];
export type FieldType = Scalar | ArrayType | Bits | Bytes | Flex | AnyLayout | (() => AnyLayout);
export type Field = readonly [name: string, offset: number, type: FieldType];

export type Source = ArrayBuffer | ArrayBufferView;

// What `write` accepts: any subset of the members, nested objects included (unions take one member).
export type WriteValue<T> = T extends bigint | number | string | boolean | Uint8Array | Int8Array | readonly unknown[] ? T : { [K in keyof T]?: WriteValue<T[K]> };

export interface Layout<T> {
  readonly name: string;
  readonly size: number;
  readonly align: number;
  readonly union: boolean;
  readonly fields: readonly Field[];
  // Decodes a record at `offset` bytes into `source`.
  read(source: Source, offset?: number): T;
  // Encodes the given members (all of a struct's; any subset of a union's) at `offset`.
  write(target: Source, value: WriteValue<T>, offset?: number): void;
  // A zeroed buffer of `size + extra` bytes (`extra` is the flexible array tail, if any).
  zeroed(extra?: number): Uint8Array;
  // A new zeroed buffer holding `value`.
  encode(value: WriteValue<T>, extra?: number): Uint8Array;
}

const SCALAR_SIZE: Record<Scalar, number> = { i8: 1, u8: 1, char: 1, bool: 1, i16: 2, u16: 2, i32: 4, u32: 4, f32: 4, i64: 8, u64: 8, f64: 8, ptr: 8 };

const decoder = new TextDecoder();
const encoder = new TextEncoder();

function dataView(source: Source): DataView {
  if (source instanceof DataView) return source;
  if (source instanceof ArrayBuffer) return new DataView(source);
  return new DataView(source.buffer, source.byteOffset, source.byteLength);
}

function sizeOf(type: FieldType): number {
  if (typeof type === "string") return SCALAR_SIZE[type];
  if (typeof type === "function") return type().size;
  if (Array.isArray(type)) return sizeOf(type[0]) * type[1];
  if ("k" in type) {
    if (type.k === "bytes") return type.n;
    if (type.k === "flex") return 0;
    throw new Error("a bitfield has no standalone size");
  }
  return (type as AnyLayout).size;
}

function readScalar(view: DataView, at: number, scalar: Scalar): number | bigint | boolean {
  switch (scalar) {
    case "i8":
    case "char":
      return view.getInt8(at);
    case "u8":
      return view.getUint8(at);
    case "bool":
      return view.getUint8(at) !== 0;
    case "i16":
      return view.getInt16(at, true);
    case "u16":
      return view.getUint16(at, true);
    case "i32":
      return view.getInt32(at, true);
    case "u32":
      return view.getUint32(at, true);
    case "f32":
      return view.getFloat32(at, true);
    case "f64":
      return view.getFloat64(at, true);
    case "i64":
      return view.getBigInt64(at, true);
    case "u64":
    case "ptr":
      return view.getBigUint64(at, true);
  }
}

function writeScalar(view: DataView, at: number, scalar: Scalar, value: unknown): void {
  switch (scalar) {
    case "i8":
    case "char":
      view.setInt8(at, Number(value));
      return;
    case "u8":
      view.setUint8(at, Number(value));
      return;
    case "bool":
      view.setUint8(at, value ? 1 : 0);
      return;
    case "i16":
      view.setInt16(at, Number(value), true);
      return;
    case "u16":
      view.setUint16(at, Number(value), true);
      return;
    case "i32":
      view.setInt32(at, Number(value), true);
      return;
    case "u32":
      view.setUint32(at, Number(value), true);
      return;
    case "f32":
      view.setFloat32(at, Number(value), true);
      return;
    case "f64":
      view.setFloat64(at, Number(value), true);
      return;
    case "i64":
      view.setBigInt64(at, BigInt(value as bigint | number), true);
      return;
    case "u64":
    case "ptr":
      view.setBigUint64(at, BigInt(value as bigint | number), true);
      return;
  }
}

function readBits(view: DataView, base: number, bits: Bits): number | bigint {
  const first = bits.bit >> 3;
  const last = (bits.bit + bits.n - 1) >> 3;
  let raw = 0n;
  for (let index = last; index >= first; index--) raw = (raw << 8n) | BigInt(view.getUint8(base + index));
  raw = (raw >> BigInt(bits.bit & 7)) & ((1n << BigInt(bits.n)) - 1n);
  const signed = bits.t === "i8" || bits.t === "i16" || bits.t === "i32" || bits.t === "i64" || bits.t === "char";
  if (signed && raw >> BigInt(bits.n - 1) !== 0n) raw -= 1n << BigInt(bits.n);
  return SCALAR_SIZE[bits.t] === 8 ? raw : Number(raw);
}

function writeBits(view: DataView, base: number, bits: Bits, value: unknown): void {
  const first = bits.bit >> 3;
  const last = (bits.bit + bits.n - 1) >> 3;
  const shift = BigInt(bits.bit & 7);
  const mask = ((1n << BigInt(bits.n)) - 1n) << shift;
  const encoded = (BigInt.asUintN(bits.n, BigInt(value as bigint | number)) << shift) & mask;
  for (let index = first; index <= last; index++) {
    const byteMask = Number((mask >> BigInt((index - first) * 8)) & 0xffn);
    const byteValue = Number((encoded >> BigInt((index - first) * 8)) & 0xffn);
    view.setUint8(base + index, (view.getUint8(base + index) & ~byteMask) | byteValue);
  }
}

function readType(view: DataView, at: number, type: FieldType, recordBase: number): unknown {
  if (typeof type === "string") return readScalar(view, at, type);
  if (typeof type === "function") return type().read(view, at);
  if (Array.isArray(type)) {
    const [element, count] = type as ArrayType;
    const step = sizeOf(element);
    if (element === "char") return readCString(view, at, count);
    if (element === "u8") return new Uint8Array(view.buffer.slice(view.byteOffset + at, view.byteOffset + at + count));
    if (element === "i8") return new Int8Array(view.buffer.slice(view.byteOffset + at, view.byteOffset + at + count));
    const items: unknown[] = [];
    for (let index = 0; index < count; index++) items.push(readType(view, at + index * step, element, recordBase));
    return items;
  }
  if ("k" in type) {
    if (type.k === "bits") return readBits(view, recordBase, type);
    if (type.k === "bytes") return new Uint8Array(view.buffer.slice(view.byteOffset + at, view.byteOffset + at + type.n));
    const step = sizeOf(type.of);
    const count = Math.max(0, Math.floor((view.byteLength - at) / step));
    return readType(view, at, [type.of, count], recordBase);
  }
  return (type as AnyLayout).read(view, at);
}

function readCString(view: DataView, at: number, count: number): string {
  const bytes = new Uint8Array(view.buffer, view.byteOffset + at, count);
  const end = bytes.indexOf(0);
  return decoder.decode(end < 0 ? bytes : bytes.subarray(0, end));
}

function writeType(view: DataView, at: number, type: FieldType, value: unknown, recordBase: number, label: string): void {
  if (typeof type === "string") return writeScalar(view, at, type, value);
  if (typeof type === "function") return type().write(view, value as never, at);
  if (Array.isArray(type)) {
    const [element, count] = type as ArrayType;
    const step = sizeOf(element);
    if (element === "char" && typeof value === "string") {
      const encoded = encoder.encode(value);
      if (encoded.length > count) throw new RangeError(`${label}: ${encoded.length} bytes do not fit char[${count}]`);
      new Uint8Array(view.buffer, view.byteOffset + at, count).fill(0).set(encoded);
      return;
    }
    const items = value as ArrayLike<unknown>;
    if (items.length > count) throw new RangeError(`${label}: ${items.length} elements do not fit [${count}]`);
    for (let index = 0; index < items.length; index++) writeType(view, at + index * step, element, items[index], recordBase, label);
    return;
  }
  if ("k" in type) {
    if (type.k === "bits") return writeBits(view, recordBase, type, value);
    if (type.k === "bytes") {
      const items = value as ArrayLike<number>;
      if (items.length > type.n) throw new RangeError(`${label}: ${items.length} bytes do not fit [${type.n}]`);
      new Uint8Array(view.buffer, view.byteOffset + at, type.n).fill(0).set(items);
      return;
    }
    const items = value as ArrayLike<unknown>;
    const step = sizeOf(type.of);
    for (let index = 0; index < items.length; index++) writeType(view, at + index * step, type.of, items[index], recordBase, label);
    return;
  }
  (type as AnyLayout).write(view, value as never, at);
}

export function layout<T>(name: string, size: number, align: number, fields: readonly Field[], union = false): Layout<T> {
  const self: Layout<T> = {
    name,
    size,
    align,
    union,
    fields,
    read(source, offset = 0) {
      const view = dataView(source);
      const result: Record<string, unknown> = {};
      for (const [fieldName, fieldOffset, type] of fields) result[fieldName] = readType(view, offset + fieldOffset, type, offset);
      return result as T;
    },
    write(target, value, offset = 0) {
      const view = dataView(target);
      const record = value as Record<string, unknown>;
      for (const [fieldName, fieldOffset, type] of fields) {
        const member = record[fieldName];
        if (member !== undefined) writeType(view, offset + fieldOffset, type, member, offset, `${name}.${fieldName}`);
      }
    },
    zeroed(extra = 0) {
      return new Uint8Array(size + extra);
    },
    encode(value, extra = 0) {
      const buffer = new Uint8Array(size + extra);
      self.write(buffer, value);
      return buffer;
    },
  };
  return self;
}

export function bits(t: Scalar, bit: number, n: number): Bits {
  return { k: "bits", t, bit, n };
}

export function bytes(n: number): Bytes {
  return { k: "bytes", n };
}

export function flex(of: FieldType): Flex {
  return { k: "flex", of };
}

// The byte size of one element of a flexible array member (for sizing `zeroed(extra)`).
export function elementSize(type: FieldType): number {
  return sizeOf(type);
}
