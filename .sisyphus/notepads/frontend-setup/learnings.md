# T3: TailwindCSS v4 + Vite config

## Summary
All three files were already correctly configured. No edits were needed:
- **vite.config.ts** — already had proxy for `/api` and `/ws` (ws:true), plus rolldownOptions for monaco-editor manualChunks
- **src/index.css** — already had `@import "tailwindcss";` as first line
- **src/main.tsx** — already imported `./index.css`

## Verification
- `bun run dev` starts cleanly: VITE v8.1.2 ready in ~165ms
- Browser console: 0 errors, 0 warnings, only a React DevTools info message
- Evidence screenshot saved to `.sisyphus/evidence/task-3-tw.png`
