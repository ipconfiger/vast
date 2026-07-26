import { memo } from 'react'
import Markdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import remarkBreaks from 'remark-breaks'
import rehypeSanitize, { defaultSchema } from 'rehype-sanitize'
import type { Components } from 'react-markdown'

// rehype-sanitize schema: default + allow className/target/rel/language-* needed for our styling.
// Default already blocks script/iframe/event-handlers/javascript: URLs.
const chatSanitizeSchema = {
  ...defaultSchema,
  attributes: {
    ...defaultSchema.attributes,
    '*': [...(defaultSchema.attributes?.['*'] || []), 'className'],
    a: [...(defaultSchema.attributes?.a || []), 'target', 'rel'],
    code: [...(defaultSchema.attributes?.code || []), ['className', /^language-\w+$/]],
    pre: ['className'],
    span: ['className'],
  },
  protocols: {
    ...defaultSchema.protocols,
    href: ['http', 'https', 'mailto'],
    src: ['http', 'https'],
  },
  tagNames: defaultSchema.tagNames?.filter(
    (t: string) => !['script', 'iframe', 'object', 'embed', 'base', 'style', 'form', 'input'].includes(t),
  ),
}

const components: Components = {
  a({ href, children, ...props }) {
    const isExternal = href?.startsWith('http')
    return (
      <a
        href={href}
        target={isExternal ? '_blank' : undefined}
        rel={isExternal ? 'noopener noreferrer' : undefined}
        className="text-blue-400 hover:text-blue-300 underline underline-offset-2"
        {...props}
      >
        {children}
      </a>
    )
  },
  // Inline vs block code: react-markdown v9+ removed the `inline` prop. Block code has a
  // `language-*` className and is wrapped in <pre>; inline code has no className.
  code({ className, children, ...props }) {
    const match = /language-(\w+)/.exec(className || '')
    if (match) {
      return <code className="block text-sm font-mono leading-relaxed" {...props}>{children}</code>
    }
    return <code className="bg-zinc-800 px-1.5 py-0.5 rounded text-sm font-mono text-emerald-300" {...props}>{children}</code>
  },
  pre({ children, ...props }) {
    return (
      <pre className="bg-zinc-900 rounded-lg p-3 my-1.5 overflow-x-auto text-sm font-mono text-zinc-200 border border-zinc-800" {...props}>
        {children}
      </pre>
    )
  },
  // Headings — compact for chat (h1 not huge)
  h1: ({ children, ...p }) => <h1 className="text-base font-bold text-zinc-100 mt-2 mb-1" {...p}>{children}</h1>,
  h2: ({ children, ...p }) => <h2 className="text-sm font-semibold text-zinc-100 mt-2 mb-1" {...p}>{children}</h2>,
  h3: ({ children, ...p }) => <h3 className="text-sm font-semibold text-zinc-100 mt-1.5 mb-0.5" {...p}>{children}</h3>,
  h4: ({ children, ...p }) => <h4 className="text-sm font-medium text-zinc-200 mt-1 mb-0.5" {...p}>{children}</h4>,
  h5: ({ children, ...p }) => <h5 className="text-xs font-medium text-zinc-300 mt-1 mb-0.5" {...p}>{children}</h5>,
  h6: ({ children, ...p }) => <h6 className="text-xs font-medium text-zinc-400 mt-1 mb-0.5" {...p}>{children}</h6>,
  p: ({ children, ...p }) => <p className="text-sm text-zinc-300 leading-relaxed my-1 first:mt-0 last:mb-0" {...p}>{children}</p>,
  ul: ({ children, ...p }) => <ul className="list-disc list-inside text-sm text-zinc-300 my-1 space-y-0.5" {...p}>{children}</ul>,
  ol: ({ children, ...p }) => <ol className="list-decimal list-inside text-sm text-zinc-300 my-1 space-y-0.5" {...p}>{children}</ol>,
  li: ({ children, ...p }) => <li className="text-sm text-zinc-300 leading-relaxed" {...p}>{children}</li>,
  blockquote: ({ children, ...p }) => <blockquote className="border-l-2 border-zinc-600 pl-3 my-1.5 text-sm text-zinc-400 italic" {...p}>{children}</blockquote>,
  hr: (p) => <hr className="border-zinc-700 my-2" {...p} />,
  strong: ({ children, ...p }) => <strong className="font-semibold text-zinc-100" {...p}>{children}</strong>,
  em: ({ children, ...p }) => <em className="italic text-zinc-200" {...p}>{children}</em>,
  table: ({ children, ...p }) => <div className="overflow-x-auto my-1.5"><table className="min-w-full text-sm text-zinc-300 border-collapse" {...p}>{children}</table></div>,
  thead: ({ children, ...p }) => <thead className="bg-zinc-800" {...p}>{children}</thead>,
  tbody: ({ children, ...p }) => <tbody className="divide-y divide-zinc-700" {...p}>{children}</tbody>,
  tr: ({ children, ...p }) => <tr className="hover:bg-zinc-800/50" {...p}>{children}</tr>,
  th: ({ children, ...p }) => <th className="px-2.5 py-1.5 text-left text-xs font-semibold text-zinc-200 uppercase tracking-wider" {...p}>{children}</th>,
  td: ({ children, ...p }) => <td className="px-2.5 py-1.5 text-sm text-zinc-300" {...p}>{children}</td>,
}

const ChatMarkdown = memo(function ChatMarkdown({ text }: { text: string }) {
  if (!text) return null
  return (
    <Markdown
      remarkPlugins={[remarkGfm, remarkBreaks]}
      rehypePlugins={[[rehypeSanitize, chatSanitizeSchema]]}
      components={components}
    >
      {text}
    </Markdown>
  )
})

export default ChatMarkdown
