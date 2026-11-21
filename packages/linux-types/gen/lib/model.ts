// The type model read from BTF: records with exact layout, enums, and the TS export names.
//
// BTF comes from `pahole -J` on the DWARF clang produced for the target arch, dumped by
// `bpftool -j btf dump`. BTF carries sizes and member offsets (bit offsets for bitfields) but not
// alignment; alignment is evaluated by the compiler (`_Alignof`) and filled in by the caller.
import { STUB_TYPES } from "./tu.ts";

export type Scalar = "i8" | "u8" | "char" | "i16" | "u16" | "i32" | "u32" | "i64" | "u64" | "f32" | "f64" | "ptr" | "bool";

export type FieldType =
  | { k: "scalar"; t: Scalar }
  | { k: "array"; of: FieldType; n: number }
  | { k: "record"; key: string }
  | { k: "inline"; rec: RecordDef }
  | { k: "bits"; t: Scalar; bit: number; n: number }
  | { k: "bytes"; n: number };

export interface FieldDef {
  name: string;
  offset: number;
  type: FieldType;
}

export interface RecordDef {
  key: string;
  cname: string;
  union: boolean;
  size: number;
  align: number;
  fields: FieldDef[];
  fromLibcStub: boolean;
}

export interface EnumDef {
  key: string;
  cname: string;
  size: number;
  signed: boolean;
  values: { name: string; value: bigint }[];
}

export interface Model {
  records: Map<string, RecordDef>;
  enums: Map<string, EnumDef>;
  // typedef_<name> exports that alias a record or enum key.
  aliases: Map<string, string>;
  // Every enumerator of every enum (named or anonymous), in BTF order.
  enumerators: { name: string; value: bigint; enumCname: string }[];
  // Scalar typedef names (for ioctl argument types): C name to scalar.
  scalarTypedefs: Map<string, Scalar>;
}

interface BtfMember {
  name: string;
  type_id: number;
  bits_offset: number;
  bitfield_size?: number;
}

interface BtfType {
  id: number;
  kind: string;
  name: string;
  size?: number;
  encoding?: string;
  type_id?: number;
  nr_elems?: number;
  members?: BtfMember[];
  values?: { name: string; val: string }[];
  fwd_kind?: string;
}

const ANON = "(anon)";
const COMPILER_TYPEDEFS = new Set(["max_align_t"]);

export function parseBtfJson(text: string): BtfType[] {
  // Enumerator values can exceed 2^53; quote them so JSON.parse does not round them.
  const quoted = text.replace(/"val":\s*(-?\d+)/g, '"val":"$1"');
  const parsed = JSON.parse(quoted) as { types: BtfType[] };
  return parsed.types;
}

function scalarOf(type: BtfType): FieldType {
  const size = type.size ?? 0;
  const signed = type.encoding === "SIGNED";
  if (type.name === "char") return { k: "scalar", t: "char" };
  if (type.encoding === "BOOL") return { k: "scalar", t: "bool" };
  if (type.name === "float") return { k: "scalar", t: "f32" };
  if (type.name === "double") return { k: "scalar", t: "f64" };
  if (type.name.includes("long double") || type.name.includes("__int128")) return { k: "bytes", n: size };
  const table: Record<number, [Scalar, Scalar]> = { 1: ["u8", "i8"], 2: ["u16", "i16"], 4: ["u32", "i32"], 8: ["u64", "i64"] };
  const pair = table[size];
  if (!pair) throw new Error(`unsupported integer ${type.name} of size ${size}`);
  return { k: "scalar", t: signed ? pair[1] : pair[0] };
}

function enumScalar(size: number, signed: boolean): Scalar {
  const table: Record<number, [Scalar, Scalar]> = { 1: ["u8", "i8"], 2: ["u16", "i16"], 4: ["u32", "i32"], 8: ["u64", "i64"] };
  const pair = table[size];
  if (!pair) throw new Error(`unsupported enum size ${size}`);
  return signed ? pair[1] : pair[0];
}

export function buildModel(types: BtfType[]): Model {
  const byId = new Map<number, BtfType>();
  for (const type of types) byId.set(type.id, type);
  const get = (id: number): BtfType => {
    const type = byId.get(id);
    if (!type) throw new Error(`BTF type ${id} missing`);
    return type;
  };
  const QUALIFIERS = new Set(["CONST", "VOLATILE", "RESTRICT", "TYPE_TAG"]);
  // Strips qualifiers and typedefs. Id 0 is void.
  const strip = (id: number): BtfType | undefined => {
    let current = id;
    for (;;) {
      if (current === 0) return undefined;
      const type = get(current);
      if (type.kind === "TYPEDEF" || QUALIFIERS.has(type.kind)) current = type.type_id ?? 0;
      else return type;
    }
  };

  // Names: record and enum keys.
  const typedefsOf = new Map<number, string[]>();
  for (const type of types) {
    if (type.kind !== "TYPEDEF") continue;
    const direct = type.type_id ?? 0;
    if (direct === 0) continue;
    const target = get(direct);
    // Follow qualifiers only: a typedef of a typedef aliases the inner typedef's target, handled below.
    if ((target.kind === "STRUCT" || target.kind === "UNION" || target.kind === "ENUM" || target.kind === "ENUM64")) {
      const list = typedefsOf.get(direct) ?? [];
      list.push(type.name);
      typedefsOf.set(direct, list);
    }
  }
  const keyOfId = new Map<number, string>();
  const cnameOfId = new Map<number, string>();
  const claim = (id: number, key: string, cname: string): void => {
    keyOfId.set(id, key);
    cnameOfId.set(id, cname);
  };
  for (const type of types) {
    const isRecord = type.kind === "STRUCT" || type.kind === "UNION";
    const isEnum = type.kind === "ENUM" || type.kind === "ENUM64";
    if (!isRecord && !isEnum) continue;
    const word = type.kind === "STRUCT" ? "struct" : type.kind === "UNION" ? "union" : "enum";
    if (type.name !== ANON) {
      claim(type.id, `${word}_${type.name}`, `${word} ${type.name}`);
      continue;
    }
    const names = (typedefsOf.get(type.id) ?? []).filter((name) => !COMPILER_TYPEDEFS.has(name)).sort();
    if (names.length > 0) claim(type.id, `typedef_${names[0]}`, names[0] as string);
  }

  const records = new Map<string, RecordDef>();
  const enums = new Map<string, EnumDef>();
  const enumerators: Model["enumerators"] = [];
  const scalarTypedefs = new Map<string, Scalar>();

  function fieldType(id: number): FieldType {
    const direct = strip(id);
    if (!direct) throw new Error("void field");
    switch (direct.kind) {
      case "INT":
        return scalarOf(direct);
      case "PTR":
        return { k: "scalar", t: "ptr" };
      case "ENUM":
      case "ENUM64":
        return { k: "scalar", t: enumScalar(direct.size ?? 4, direct.encoding === "SIGNED") };
      case "ARRAY": {
        const elem = fieldType(direct.type_id ?? 0);
        return { k: "array", of: elem, n: direct.nr_elems ?? 0 };
      }
      case "STRUCT":
      case "UNION": {
        const key = keyOfId.get(direct.id);
        if (key) {
          ensureRecord(direct);
          return { k: "record", key };
        }
        return { k: "inline", rec: buildRecord(direct, "", "") };
      }
      case "FWD":
        throw new Error(`field of incomplete type ${direct.name}`);
      case "FUNC_PROTO":
        return { k: "scalar", t: "ptr" };
      default:
        throw new Error(`unsupported BTF kind ${direct.kind} (${direct.name})`);
    }
  }

  function buildRecord(type: BtfType, key: string, cname: string): RecordDef {
    const fields: FieldDef[] = [];
    let anonymous = 0;
    for (const member of type.members ?? []) {
      const name = member.name === ANON ? `_anon${anonymous++}` : member.name;
      if (member.bitfield_size) {
        const base = fieldType(member.type_id);
        if (base.k !== "scalar") throw new Error(`bitfield ${type.name}.${name} of non-scalar type`);
        fields.push({ name, offset: member.bits_offset >> 3, type: { k: "bits", t: base.t, bit: member.bits_offset, n: member.bitfield_size } });
        continue;
      }
      if (member.bits_offset % 8 !== 0) throw new Error(`unaligned member ${type.name}.${name}`);
      fields.push({ name, offset: member.bits_offset / 8, type: fieldType(member.type_id) });
    }
    return {
      key,
      cname,
      union: type.kind === "UNION",
      size: type.size ?? 0,
      align: 0,
      fields,
      fromLibcStub: STUB_TYPES.has(cname),
    };
  }

  const building = new Set<number>();
  function ensureRecord(type: BtfType): void {
    const key = keyOfId.get(type.id) as string;
    if (records.has(key) || building.has(type.id)) return;
    building.add(type.id);
    const record = buildRecord(type, key, cnameOfId.get(type.id) as string);
    building.delete(type.id);
    records.set(key, record);
  }

  const seenCollision = new Map<string, number>();
  for (const type of types) {
    const key = keyOfId.get(type.id);
    if (!key) continue;
    if (type.kind === "STRUCT" || type.kind === "UNION") {
      const previous = seenCollision.get(key);
      if (previous !== undefined) {
        const a = JSON.stringify(buildRecord(get(previous), key, ""));
        const b = JSON.stringify(buildRecord(type, key, ""));
        if (a !== b) throw new Error(`two different definitions for ${key} (BTF ids ${previous} and ${type.id})`);
        continue;
      }
      seenCollision.set(key, type.id);
      ensureRecord(type);
    } else {
      const previous = seenCollision.get(key);
      if (previous !== undefined) {
        if (JSON.stringify(get(previous).values) !== JSON.stringify(type.values)) throw new Error(`two different definitions for ${key}`);
        continue;
      }
      seenCollision.set(key, type.id);
    }
  }

  // Enums: named or typedef'd ones get an export; every enumerator is collected.
  for (const type of types) {
    if (type.kind !== "ENUM" && type.kind !== "ENUM64") continue;
    // bpftool prints 32-bit enumerators sign-extended to 64 bits even for unsigned enums: normalize to the enum's own width.
    const width = (type.size ?? 4) * 8;
    const normalize = (raw: bigint): bigint => (type.encoding === "SIGNED" ? BigInt.asIntN(width, raw) : BigInt.asUintN(width, raw));
    const values = (type.values ?? []).map((entry) => ({ name: entry.name, value: normalize(BigInt(entry.val)) }));
    const key = keyOfId.get(type.id);
    const cname = cnameOfId.get(type.id) ?? ANON;
    for (const entry of values) enumerators.push({ ...entry, enumCname: cname });
    if (key && !enums.has(key)) {
      enums.set(key, { key, cname, size: type.size ?? 4, signed: type.encoding === "SIGNED", values });
    }
  }

  // typedef_<name> aliases for typedefs that resolve to an exported record or enum.
  const aliases = new Map<string, string>();
  for (const type of types) {
    if (type.kind !== "TYPEDEF") continue;
    if (COMPILER_TYPEDEFS.has(type.name)) continue;
    const target = strip(type.id);
    if (!target) continue;
    const key = keyOfId.get(target.id);
    const aliasKey = `typedef_${type.name}`;
    if (key && key !== aliasKey) aliases.set(aliasKey, key);
    else if (!key && target.kind === "INT") {
      const scalar = scalarOf(target);
      if (scalar.k === "scalar") scalarTypedefs.set(type.name, scalar.t);
    } else if (!key && target.kind === "PTR") scalarTypedefs.set(type.name, "ptr");
  }

  return { records, enums, aliases, enumerators, scalarTypedefs };
}

// Alignment of a field type given the compiler-evaluated alignment of every keyed record.
function alignOfType(type: FieldType, model: Model): number {
  switch (type.k) {
    case "scalar":
    case "bits":
      return scalarSize(type.t);
    case "array":
      return alignOfType(type.of, model);
    case "inline":
      return alignOfInline(type.rec, model);
    case "bytes":
      return Math.min(16, type.n);
    case "record":
      return (model.records.get(type.key) as RecordDef).align;
  }
}

function alignOfInline(record: RecordDef, model: Model): number {
  if (record.align === 0) record.align = Math.max(1, ...record.fields.map((field) => alignOfType(field.type, model)));
  return record.align;
}

// Sets every inline record's alignment from its members (records with a key must already have the
// compiler's value); anonymous inline records have no name the compiler could be asked about.
export function fillInlineAlignments(model: Model): void {
  const visit = (type: FieldType): void => {
    if (type.k === "array") visit(type.of);
    else if (type.k === "inline") {
      for (const field of type.rec.fields) visit(field.type);
      alignOfInline(type.rec, model);
    }
  };
  for (const record of model.records.values()) for (const field of record.fields) visit(field.type);
}

export function scalarSize(scalar: Scalar): number {
  switch (scalar) {
    case "i8":
    case "u8":
    case "char":
    case "bool":
      return 1;
    case "i16":
    case "u16":
      return 2;
    case "i32":
    case "u32":
    case "f32":
      return 4;
    default:
      return 8;
  }
}
