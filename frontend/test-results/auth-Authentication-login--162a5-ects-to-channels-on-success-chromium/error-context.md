# Page snapshot

```yaml
- generic [ref=e5]:
  - generic [ref=e6]:
    - img [ref=e8]
    - heading "Welcome back" [level=1] [ref=e11]
    - paragraph [ref=e12]: Sign in to your account
  - paragraph [ref=e14]: Session expired. Please log in again.
  - generic [ref=e15]:
    - generic [ref=e16]:
      - generic [ref=e17]: Username
      - generic [ref=e18]:
        - img [ref=e19]
        - textbox "Username" [ref=e22]: e2elogintest
    - generic [ref=e23]:
      - generic [ref=e24]: Password
      - generic [ref=e25]:
        - img [ref=e26]
        - textbox "Password" [ref=e29]: E2eLogin123
    - button "Sign in" [ref=e30]
  - paragraph [ref=e31]:
    - text: Don't have an account?
    - link "Create one" [ref=e32] [cursor=pointer]:
      - /url: /register
```