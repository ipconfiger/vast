# Task 4 Learnings

## React Router v7
- `createBrowserRouter` + `RouterProvider` from `react-router` (v7) — no need for `react-router-dom`
- Route paths: `/login`, `/register`, `/channels`, `/channels/:channelId`, `/channels/:channelId/thread/:messageId`, `/dm/:userId`, `/search`

## Zustand with TypeScript
- `zustand/middleware` provides `persist` — uses `localStorage` under `auth-storage` key
- `useAuthStore.getState().token` for reading token outside React components (e.g., in apiClient)
- Maps in Zustand require spreading into new Map on every update for reactivity
- `noUnusedLocals` + `noUnusedParameters` are strict — every symbol must be used

## TypeScript 6.0
- `baseUrl` is deprecated; `ignoreDeprecations: "6.0"` silences TS5101
- `tsc --noEmit` still passes clean with this setting
