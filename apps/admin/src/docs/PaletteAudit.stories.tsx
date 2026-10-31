import type { Meta, StoryObj } from "@storybook/react-vite";
import sheets from "./paletteSources";

/** Every literal colour in the stylesheet, with the files that use it. */
function PaletteAudit() {
  const uses = new Map<string, Set<string>>();
  const tokens = new Map<string, string>();
  for (const [file, css] of Object.entries(sheets)) {
    for (const m of css.matchAll(/(--color-[\w-]+):\s*(#[0-9a-fA-F]{3,8})\b/g)) tokens.set(m[2].toLowerCase(), m[1]);
    if (file.includes("tokens")) continue;
    for (const m of css.matchAll(/#[0-9a-fA-F]{3,8}\b/g)) {
      const c = m[0].toLowerCase();
      if (!uses.has(c)) uses.set(c, new Set());
      uses.get(c)!.add(file.replace(/^.*\/ui\//, ""));
    }
  }
  const rows = [...uses.entries()].sort((a, b) => b[1].size - a[1].size || a[0].localeCompare(b[0]));
  return (
    <div style={{ fontSize: 12 }}>
      <p style={{ maxWidth: 720, color: "var(--color-text)", marginBottom: 16 }}>
        {tokens.size} colours are named tokens. Component partials contain {rows.length} literal colours
        {rows.length === 0 ? " — every colour comes from a token." : "; replace them with the nearest token."}
      </p>
      <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(220px, 1fr))", gap: 8 }}>
        {rows.map(([color, files]) => (
          <div key={color} style={{ display: "flex", gap: 8, alignItems: "center", background: "var(--color-surface)", border: "1px solid var(--color-border)", borderRadius: 7, padding: 8 }}>
            <span style={{ width: 28, height: 28, borderRadius: 5, background: color, border: "1px solid #0001", flex: "none" }} />
            <span>
              <code>{color}</code>
              <br />
              <span style={{ color: "var(--color-text-muted)" }}>{[...files].join(", ")}</span>
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}

const meta = {
  title: "Foundations/Palette audit",
  component: PaletteAudit,
  parameters: { docs: { description: { component: "Computed live from the CSS partials." } } },
} satisfies Meta<typeof PaletteAudit>;
export default meta;
export const Audit: StoryObj<typeof meta> = {};
