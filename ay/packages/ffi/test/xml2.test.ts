import { describe, test, expect } from "bun:test";
import { parseXml, parseXmlFile } from "../src/xml2.ts";
import { tmpdir } from "node:os";
import { join } from "node:path";

describe("xml2 (libxml2): parseXml / parseXmlFile", () => {
  // 1. Parse a simple XML: root.name === "root"
  test("simple XML: root.name === 'root'", () => {
    const root = parseXml("<root><child>text</child></root>");
    expect(root.name).toBe("root");
  });

  // 2. Get the text of a child
  test("text content of a child", () => {
    const root = parseXml("<root><child>hello world</child></root>");
    expect(root.children).toHaveLength(1);
    expect(root.children[0].name).toBe("child");
    expect(root.children[0].content).toBe("hello world");
  });

  // 3. Read an attribute
  test("getAttribute: <item id='42'/> -> getAttribute('id') === '42'", () => {
    const root = parseXml('<root><item id="42" type="foo"/></root>');
    const item = root.children[0];
    expect(item.getAttribute("id")).toBe("42");
    expect(item.getAttribute("type")).toBe("foo");
    expect(item.getAttribute("missing")).toBeNull();
  });

  // 4. Multi-level (3 levels deep)
  test("tree 3 levels deep", () => {
    const xml = `
      <a>
        <b>
          <c>deep</c>
        </b>
      </a>
    `;
    const root = parseXml(xml);
    expect(root.name).toBe("a");
    expect(root.children[0].name).toBe("b");
    expect(root.children[0].children[0].name).toBe("c");
    expect(root.children[0].children[0].content).toBe("deep");
  });

  // 5. XML with namespaces must not crash
  test("XML with namespaces does not crash", () => {
    const xml = `<?xml version="1.0"?>
<root xmlns:ns="http://example.com/ns">
  <ns:child ns:attr="value">text</ns:child>
</root>`;
    expect(() => parseXml(xml)).not.toThrow();
    const root = parseXml(xml);
    expect(root.name).toBe("root");
  });

  // 6. Invalid XML throws an explicit error
  test("invalid XML -> throws Error", () => {
    expect(() => parseXml("<unclosed")).toThrow();
    expect(() => parseXml("not xml at all")).toThrow();
  });

  // 7. Parse a file on disk
  test("parseXmlFile: temporary XML file", async () => {
    const tmpPath = join(tmpdir(), `ay-xml2-test-${Date.now()}.xml`);
    await Bun.write(tmpPath, `<config><host>localhost</host><port>8080</port></config>`);

    const root = parseXmlFile(tmpPath);
    expect(root.name).toBe("config");
    expect(root.children).toHaveLength(2);
    expect(root.children[0].name).toBe("host");
    expect(root.children[0].content).toBe("localhost");
    expect(root.children[1].name).toBe("port");
    expect(root.children[1].content).toBe("8080");
  });

  // Bonus: missing file throws
  test("parseXmlFile on a missing file -> throws Error", () => {
    expect(() => parseXmlFile(join(tmpdir(), "nonexistent-ay-xml2.xml"))).toThrow("cannot read");
  });

  // Bonus: several children
  test("several children at the same level", () => {
    const xml = "<ul><li>A</li><li>B</li><li>C</li></ul>";
    const root = parseXml(xml);
    expect(root.children).toHaveLength(3);
    expect(root.children.map((c) => c.content)).toEqual(["A", "B", "C"]);
  });

  test("invalid XML error message is in English", () => {
    expect(() => parseXml("<unclosed")).toThrow("invalid XML or parse error");
  });

  test("elements without text have null content", () => {
    const root = parseXml("<a><b/></a>");
    expect(root.content).toBeNull();
    expect(root.children[0].content).toBeNull();
  });
});
