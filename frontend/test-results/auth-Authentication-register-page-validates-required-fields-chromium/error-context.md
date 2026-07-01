# Page snapshot

```yaml
- generic [ref=e5]:
  - generic [ref=e6]:
    - img [ref=e8]
    - heading "Create account" [level=1] [ref=e11]
    - paragraph [ref=e12]: Join the conversation
  - generic [ref=e13]:
    - generic [ref=e14]:
      - generic [ref=e15]: Username
      - generic [ref=e16]:
        - img [ref=e17]
        - textbox "Username" [active] [ref=e20]
    - generic [ref=e21]:
      - generic [ref=e22]: Password
      - generic [ref=e23]:
        - img [ref=e24]
        - textbox "Password" [ref=e27]
    - generic [ref=e28]:
      - generic [ref=e29]: Invite Code
      - generic [ref=e30]:
        - img [ref=e31]
        - textbox "Invite Code" [ref=e34]
    - button "Create account" [ref=e35]
  - paragraph [ref=e36]:
    - text: Already have an account?
    - link "Sign in" [ref=e37] [cursor=pointer]:
      - /url: /login
```