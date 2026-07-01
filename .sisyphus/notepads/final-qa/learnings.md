
## F3: Real Manual QA - 2026-07-01

### Patterns
- API message format uses `msg_type` + `payload` (not `content`), where `payload` is a JSON value containing `{"text":"..."}`
- API channels endpoint returns `{"channels": [...]}` wrapped object, not a raw array
- apiClient<T> returns `response.json()` directly - no unwrapping

### Successful Approaches
- Running cargo in background with `&>` redirect for server logs
- Sequential curl testing with token extraction via python3 one-liners
- Playwright MCP for frontend form verification (snapshot + screenshot)
