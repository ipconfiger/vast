interface TextMessageProps {
  text: string
}

export function TextMessage({ text }: TextMessageProps) {
  const parts = text.split(/(@\w+)/g)

  return (
    <span className="text-message whitespace-pre-wrap break-words">
      {parts.map((part, i) => {
        if (part.startsWith('@')) {
          return (
            <span key={i} className="mention font-semibold">
              {part}
            </span>
          )
        }
        return <span key={i}>{part}</span>
      })}
    </span>
  )
}
