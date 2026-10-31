// Dependency-free generator for the gateway's OpenAPI 3.1 schema subset.
import { writeFile } from "node:fs/promises";
const url = process.argv[2] || "http://127.0.0.1:8080/openapi.json";
const response = await fetch(url);
if (!response.ok) throw new Error(`OpenAPI HTTP ${response.status}`);
const api = await response.json();
function type(s) {
  if (!s || Object.keys(s).length === 0) return "unknown";
  if (s.$ref) return s.$ref.split("/").at(-1);
  if (s.const !== undefined) return JSON.stringify(s.const);
  if (s.enum) return s.enum.map((v) => JSON.stringify(v)).join(" | ");
  if (s.oneOf || s.anyOf) return (s.oneOf || s.anyOf).map(type).join(" | ");
  if (s.allOf) return s.allOf.map((v) => `(${type(v)})`).join(" & ");
  if (Array.isArray(s.type))
    return s.type.map((t) => type({ ...s, type: t })).join(" | ");
  if (s.type === "array") return `Array<${type(s.items)}>`;
  if (s.type === "object" || s.properties) {
    const fields = Object.entries(s.properties || {}).map(
      ([k, v]) =>
        `  ${JSON.stringify(k)}${s.required?.includes(k) ? "" : "?"}: ${type(v)};`,
    );
    if (s.additionalProperties)
      fields.push(
        `  [key: string]: ${type(s.additionalProperties === true ? {} : s.additionalProperties)};`,
      );
    return `{\n${fields.join("\n")}\n}`;
  }
  return (
    {
      integer: "number",
      number: "number",
      boolean: "boolean",
      string: "string",
      null: "null",
    }[s.type] || "unknown"
  );
}
const schemas = Object.entries(api.components.schemas).sort(([a], [b]) =>
  a.localeCompare(b),
);
const output =
  "// Generated from the running gateway /openapi.json. Do not hand edit.\n" +
  schemas
    .map(([name, schema]) => `export type ${name} = ${type(schema)};\n`)
    .join("\n");
await writeFile(new URL("../src/schema.ts", import.meta.url), output);
console.log(`Generated ${schemas.length} schema types from ${url}`);
