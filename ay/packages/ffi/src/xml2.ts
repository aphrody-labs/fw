// bun:ffi binding to libxml2.so.16: XML parsing without an npm dependency.
//
// xmlNode ABI (x86_64, libxml2.so.16, confirmed by xmlnode_offset.c):
//   offset  0 : _private     (ptr,  8 bytes)
//   offset  8 : type          (int,  4 bytes) - XML_ELEMENT_NODE=1, XML_TEXT_NODE=3
//   offset 16 : name          (const xmlChar*, 8 bytes)
//   offset 24 : children      (xmlNodePtr, 8 bytes)
//   offset 40 : parent        (xmlNodePtr, 8 bytes)
//   offset 48 : next          (xmlNodePtr, 8 bytes)
//   offset 56 : prev          (xmlNodePtr, 8 bytes)
//   offset 64 : doc           (xmlDocPtr, 8 bytes)
//   offset 80 : content       (xmlChar*, 8 bytes)
//   offset 88 : properties    (xmlAttrPtr, 8 bytes)
//
// xmlAttr ABI (sizeof = 104):
//   offset  8 : type          (int, 4 bytes)
//   offset 16 : name          (const xmlChar*, 8 bytes)
//   offset 24 : children      (xmlNodePtr, the text node carrying the value)
//   offset 40 : parent        (xmlNodePtr)
//   offset 48 : next          (xmlAttrPtr, next attribute)
//   offset 64 : doc           (xmlDocPtr)
//
// Reading strategy: `read.ptr(addr, offset)` and `read.i32(addr, offset)` from bun:ffi;
// every pointer is a `number` in Bun.

import { dlopen, FFIType, ptr, read, CString } from "bun:ffi";

// ── ABI constants ─────────────────────────────────────────────────────────────

const XML_ELEMENT_NODE = 1;

// xmlNode offsets
const OFF_NODE_TYPE = 8;
const OFF_NODE_NAME = 16;
const OFF_NODE_PROPERTIES = 88;

// xmlAttr offsets
const OFF_ATTR_NAME = 16;
const OFF_ATTR_CHILDREN = 24;
const OFF_ATTR_NEXT = 48;

// ── FFI ──────────────────────────────────────────────────────────────────────

const lib = dlopen("libxml2.so.16", {
  // xmlDocPtr xmlParseMemory(const char *buffer, int size)
  xmlParseMemory: { args: [FFIType.ptr, FFIType.i32], returns: FFIType.ptr },
  // xmlDocPtr xmlReadFile(const char *URL, const char *encoding, int options)
  xmlReadFile: { args: [FFIType.ptr, FFIType.ptr, FFIType.i32], returns: FFIType.ptr },
  // xmlNodePtr xmlDocGetRootElement(xmlDocPtr doc)
  xmlDocGetRootElement: { args: [FFIType.ptr], returns: FFIType.ptr },
  // xmlChar* xmlNodeGetContent(xmlNodePtr node)
  xmlNodeGetContent: { args: [FFIType.ptr], returns: FFIType.ptr },
  // xmlChar* xmlGetProp(xmlNodePtr node, const xmlChar *name)
  xmlGetProp: { args: [FFIType.ptr, FFIType.ptr], returns: FFIType.ptr },
  // void xmlFreeDoc(xmlDocPtr doc)
  xmlFreeDoc: { args: [FFIType.ptr], returns: FFIType.void },
  // void xmlCleanupParser(void)
  xmlCleanupParser: { args: [], returns: FFIType.void },
  // xmlChar* xmlNodeListGetString(xmlDocPtr doc, xmlNodePtr list, int inLine)
  xmlNodeListGetString: { args: [FFIType.ptr, FFIType.ptr, FFIType.i32], returns: FFIType.ptr },
  // int xmlChildElementCount(xmlNodePtr node)
  xmlChildElementCount: { args: [FFIType.ptr], returns: FFIType.i32 },
  // xmlNodePtr xmlFirstElementChild(xmlNodePtr node)
  xmlFirstElementChild: { args: [FFIType.ptr], returns: FFIType.ptr },
  // xmlNodePtr xmlNextElementSibling(xmlNodePtr node)
  xmlNextElementSibling: { args: [FFIType.ptr], returns: FFIType.ptr },
});

const sym = lib.symbols;

// ── Helpers ──────────────────────────────────────────────────────────────────

/** Encodes a JS string as a NUL-terminated UTF-8 buffer. */
function encodeStr(s: string): Uint8Array {
  const bytes = new TextEncoder().encode(s);
  const buf = new Uint8Array(bytes.length + 1);
  buf.set(bytes);
  return buf;
}

// ── Public interface ──────────────────────────────────────────────────────────

export interface XmlNode {
  /** Tag name (e.g. "root", "item"). */
  name: string;
  /** Text content (null when there is no direct text node). */
  content: string | null;
  /** Element children only (not the raw text nodes). */
  children: XmlNode[];
  /** Returns the value of an attribute, or null when it does not exist. */
  getAttribute(name: string): string | null;
  /** @internal Attribute map (name to value). */
  _attrs: Map<string, string>;
}

// ── Tree construction ─────────────────────────────────────────────────────────

/**
 * Recursively builds an XmlNode from a libxml2 xmlNodePtr.
 * The node must be of type XML_ELEMENT_NODE.
 *
 * Uses `read.ptr(addr, offset)` / `read.i32(addr, offset)`: every libxml2 pointer is a `number` in Bun.
 */
function buildNode(nodeAddr: number, docAddr: number): XmlNode {
  // Node name (name field at offset 16)
  const nameAddr = read.ptr(nodeAddr, OFF_NODE_NAME);
  const name = nameAddr ? new CString(nameAddr).toString() : "(unknown)";

  // Text content through xmlNodeGetContent (concatenates the text nodes)
  const rawContentAddr = sym.xmlNodeGetContent(nodeAddr);
  let content: string | null = null;
  if (rawContentAddr) {
    const raw = new CString(rawContentAddr).toString();
    // Ignore blank content (libxml2 returns whitespace for purely structural elements)
    if (raw.trim().length > 0) content = raw;
  }

  // Attributes: walk the xmlAttr linked list through `properties` (offset 88)
  const attrs = new Map<string, string>();
  let attrAddr = read.ptr(nodeAddr, OFF_NODE_PROPERTIES);
  while (attrAddr) {
    const attrNameAddr = read.ptr(attrAddr, OFF_ATTR_NAME);
    const attrName = attrNameAddr ? new CString(attrNameAddr).toString() : null;

    // Attribute value: text node child of the xmlAttr (offset 24)
    const attrChildAddr = read.ptr(attrAddr, OFF_ATTR_CHILDREN);
    let attrVal: string | null = null;
    if (attrChildAddr) {
      const valAddr = sym.xmlNodeListGetString(docAddr, attrChildAddr, 1);
      if (valAddr) attrVal = new CString(valAddr).toString();
    }

    if (attrName && attrVal !== null) {
      attrs.set(attrName, attrVal);
    }

    // Next attribute (offset 48)
    attrAddr = read.ptr(attrAddr, OFF_ATTR_NEXT);
  }

  // Element children: xmlFirstElementChild + xmlNextElementSibling
  const children: XmlNode[] = [];
  let childAddr = sym.xmlFirstElementChild(nodeAddr);
  while (childAddr) {
    // Check that it is an XML_ELEMENT_NODE (type at offset 8)
    const childType = read.i32(childAddr, OFF_NODE_TYPE);
    if (childType === XML_ELEMENT_NODE) {
      children.push(buildNode(childAddr, docAddr));
    }
    childAddr = sym.xmlNextElementSibling(childAddr);
  }

  const node: XmlNode = {
    name,
    content,
    children,
    _attrs: attrs,
    getAttribute(attrName: string): string | null {
      return this._attrs.get(attrName) ?? null;
    },
  };

  return node;
}

// ── Public API ────────────────────────────────────────────────────────────────

/**
 * Parses an XML string and returns the root node.
 * Throws when the XML is invalid or the document is empty.
 */
export function parseXml(xmlStr: string): XmlNode {
  const encoded = new TextEncoder().encode(xmlStr);
  const buf = new Uint8Array(encoded.length + 1);
  buf.set(encoded);

  const docAddr = sym.xmlParseMemory(ptr(buf), encoded.length);
  if (!docAddr) {
    throw new Error("parseXml: invalid XML or parse error");
  }

  try {
    const rootAddr = sym.xmlDocGetRootElement(docAddr);
    if (!rootAddr) {
      throw new Error("parseXml: document has no root element");
    }
    return buildNode(rootAddr, docAddr);
  } finally {
    sym.xmlFreeDoc(docAddr);
  }
}

/**
 * Reads and parses an XML file from disk.
 * Throws when the file is not found or invalid.
 */
export function parseXmlFile(filePath: string): XmlNode {
  const cpath = encodeStr(filePath);
  const docAddr = sym.xmlReadFile(ptr(cpath), 0, 0);
  if (!docAddr) {
    throw new Error(`parseXmlFile: cannot read "${filePath}"`);
  }

  try {
    const rootAddr = sym.xmlDocGetRootElement(docAddr);
    if (!rootAddr) {
      throw new Error(`parseXmlFile: document "${filePath}" has no root element`);
    }
    return buildNode(rootAddr, docAddr);
  } finally {
    sym.xmlFreeDoc(docAddr);
  }
}
