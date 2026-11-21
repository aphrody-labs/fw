import { describe, expect, test } from "bun:test";
import { REMOTE_SCRIPT, SECTIONS, parseSections } from "./fleet-inventory.ts";

describe("fleet-inventory", () => {
  test("every section has a marker in the remote script", () => {
    for (const s of SECTIONS) expect(REMOTE_SCRIPT).toContain(`S ${s};`);
  });

  test("parseSections splits by marker and ignores unknown markers and blanks", () => {
    const parsed = parseSections("##host\nn1\r\nUbuntu\n\n##nope\nx\n##lvm\n  data xbuild\n");
    expect(parsed.host).toEqual(["n1", "Ubuntu"]);
    expect(parsed.lvm).toEqual(["  data xbuild"]);
    expect(parsed.docker).toEqual([]);
  });
});
