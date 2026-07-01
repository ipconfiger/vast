# T18: Login/Register UI Pages — Verification Evidence

## Files Created/Modified

| File | Action | Status |
|------|--------|--------|
| `src/pages/LoginPage.tsx` | Created | ✅ |
| `src/pages/RegisterPage.tsx` | Created | ✅ |
| `src/components/AuthGuard.tsx` | Created | ✅ |
| `src/App.tsx` | Updated | ✅ |

## Verification

- `tsc --noEmit`: **PASSED** — No TypeScript errors
- `lsp_diagnostics`: **CLEAN** — No diagnostics on any new/modified files

## Implementation Details

### LoginPage.tsx
- Username + password form with `useMutation` from TanStack Query
- POST `/api/auth/login` via `apiClient`
- On success: `storeLogin(data.token, data.user)`, navigate to `/channels`
- On error: red error banner with `mutation.error.message`
- Loading state: spinner in submit button, disabled during mutation
- Dark theme (slate-950 bg, slate-900 card, indigo-600 accent)
- Lucide icons: Lock, User
- Link to `/register`

### RegisterPage.tsx
- Username + password + invite_code form with `useMutation`
- POST `/api/auth/register` via `apiClient`
- Client-side validation (username ≥3 chars, password ≥6 chars, invite code required)
- Field-level error messages with red borders + red text
- Server-side field errors (`error.errors`) mapped to fields
- On success: login store + navigate to `/channels`
- Dark theme consistent with login
- Lucide icons: Lock, User, KeyRound
- Link to `/login`
- Errors clear on input change

### AuthGuard.tsx
- Checks `useAuthStore().isAuthenticated`
- Not authenticated → `<Navigate to="/login" replace />`
- Authenticated → `<Outlet />` (renders children)
- Used as layout route in App.tsx

### App.tsx
- `/login` → `<LoginPage />` (unprotected)
- `/register` → `<RegisterPage />` (unprotected)
- All other routes wrapped in `<AuthGuard>` layout route:
  - `/channels`, `/channels/:channelId`, `/channels/:channelId/thread/:messageId`
  - `/dm/:userId`, `/search`, `/`
- `QueryClientProvider` wraps `RouterProvider`
