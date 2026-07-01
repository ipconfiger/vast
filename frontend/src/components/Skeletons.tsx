export function ChannelListSkeleton() {
  return (
    <div className="flex flex-col gap-1 px-2">
      {Array.from({ length: 6 }).map((_, i) => (
        <div
          key={i}
          className="flex items-center gap-2 rounded-md px-3 py-1.5"
        >
          <div
            className="h-4 w-4 flex-shrink-0 animate-pulse rounded bg-zinc-800"
            style={{ animationDelay: `${i * 100}ms` }}
          />
          <div
            className="h-3 flex-1 animate-pulse rounded bg-zinc-800"
            style={{ animationDelay: `${i * 100}ms` }}
          />
        </div>
      ))}
    </div>
  )
}

export function MessageListSkeleton() {
  return (
    <div className="flex flex-1 flex-col gap-6 px-6 py-4">
      {Array.from({ length: 5 }).map((_, i) => (
        <div key={i} className="flex gap-3">
          <div
            className="h-9 w-9 flex-shrink-0 animate-pulse rounded-full bg-zinc-800"
            style={{ animationDelay: `${i * 150}ms` }}
          />
          <div className="flex flex-1 flex-col gap-2">
            <div className="flex items-center gap-2">
              <div
                className="h-3 w-24 animate-pulse rounded bg-zinc-800"
                style={{ animationDelay: `${i * 150}ms` }}
              />
              <div
                className="h-2.5 w-12 animate-pulse rounded bg-zinc-800/50"
                style={{ animationDelay: `${i * 150}ms` }}
              />
            </div>
            <div
              className="h-3 w-3/4 animate-pulse rounded bg-zinc-800/60"
              style={{ animationDelay: `${i * 150}ms` }}
            />
            <div
              className="h-3 w-1/2 animate-pulse rounded bg-zinc-800/40"
              style={{ animationDelay: `${i * 150}ms` }}
            />
          </div>
        </div>
      ))}
    </div>
  )
}
