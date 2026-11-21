// Asks the compiler for the facts BTF does not carry (alignment) and cross-checks everything BTF
// does carry (size, member offsets) against sizeof and offsetof, so a decoder error fails the run.
import { evaluate, type Probe } from "./evaluate.ts";
import { fillInlineAlignments, type FieldDef, type Model, type RecordDef } from "./model.ts";
import { type Tu } from "./tu.ts";

const typeName = (record: RecordDef): string => record.cname;

export async function verifyLayouts(model: Model, tu: Tu): Promise<{ checkedSizes: number; checkedOffsets: number; invalidAlignments: number }> {
  const probes: Probe[] = [];
  for (const record of model.records.values()) {
    const name = typeName(record);
    probes.push({ key: `s:${record.key}`, expr: `sizeof(${name})` });
    probes.push({ key: `a:${record.key}`, expr: `_Alignof(${name})` });
  }
  // Offsets of named members, including those reached through anonymous members.
  const offsetProbes = new Map<string, { record: RecordDef; absolute: number }>();
  for (const record of model.records.values()) {
    const walk = (fields: FieldDef[], base: number): void => {
      for (const field of fields) {
        const absolute = base + field.offset;
        if (field.type.k === "bits") continue;
        if (field.name.startsWith("_anon")) {
          if (field.type.k === "inline") walk(field.type.rec.fields, absolute);
          continue;
        }
        offsetProbes.set(`o:${record.key}:${field.name}`, { record, absolute });
        probes.push({ key: `o:${record.key}:${field.name}`, expr: `__builtin_offsetof(${typeName(record)}, ${field.name})` });
      }
    };
    walk(record.fields, 0);
  }
  const results = await evaluate(tu.cflags, tu.sourcePath, probes, tu.workDir, "layout");
  let checkedSizes = 0;
  for (const record of model.records.values()) {
    const size = results.get(`s:${record.key}`);
    const align = results.get(`a:${record.key}`);
    if (!size || !align) throw new Error(`the compiler could not evaluate sizeof/_Alignof for ${record.cname} (${record.key})`);
    if (Number(size.value) !== record.size) throw new Error(`${record.cname}: BTF size ${record.size} but sizeof is ${size.value}`);
    record.align = Number(align.value);
    checkedSizes++;
  }
  let checkedOffsets = 0;
  for (const [key, { absolute }] of offsetProbes) {
    const result = results.get(key);
    if (!result) throw new Error(`the compiler could not evaluate ${key}`);
    if (Number(result.value) !== absolute) throw new Error(`${key}: BTF offset ${absolute} but offsetof is ${result.value}`);
    checkedOffsets++;
  }
  fillInlineAlignments(model);
  // sizeof is always a multiple of the alignment; anything else would mean a bad probe.
  let invalidAlignments = 0;
  for (const record of model.records.values()) if (record.size % record.align !== 0 && record.size !== 0) invalidAlignments++;
  return { checkedSizes, checkedOffsets, invalidAlignments };
}
