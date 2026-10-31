// Raw CSS of the design system partials, for the palette audit story.
const sheets = import.meta.glob("../ui/**/*.css", { query: "?raw", import: "default", eager: true }) as Record<string, string>;
export default sheets;
