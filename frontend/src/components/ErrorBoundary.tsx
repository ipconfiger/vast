import { Component, type ReactNode } from 'react'
import { AlertTriangle, RefreshCw } from 'lucide-react'

interface ErrorBoundaryProps {
  children: ReactNode
  fallback?: ReactNode
}

interface ErrorBoundaryState {
  hasError: boolean
  error: Error | null
}

export class ErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  constructor(props: ErrorBoundaryProps) {
    super(props)
    this.state = { hasError: false, error: null }
  }

  static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return { hasError: true, error }
  }

  handleReset = () => {
    this.setState({ hasError: false, error: null })
  }

  render() {
    if (this.state.hasError) {
      if (this.props.fallback) return this.props.fallback

      return (
        <div className="flex h-screen items-center justify-center bg-zinc-950 p-6">
          <div className="flex max-w-md flex-col items-center gap-4 text-center">
            <div className="rounded-full bg-red-900/30 p-3">
              <AlertTriangle className="h-8 w-8 text-red-400" />
            </div>
            <div>
              <h2 className="text-lg font-semibold text-zinc-100">
                Something went wrong
              </h2>
              <p className="mt-1 text-sm text-zinc-400">
                An unexpected error occurred. Please try refreshing the page.
              </p>
              {this.state.error && (
                <pre className="mt-3 max-h-32 overflow-auto rounded-md bg-zinc-900 p-3 text-left font-mono text-xs text-red-400">
                  {this.state.error.message}
                </pre>
              )}
            </div>
            <button
              onClick={this.handleReset}
              className="inline-flex items-center gap-2 rounded-md bg-zinc-800 px-4 py-2 text-sm font-medium text-zinc-200 transition-colors hover:bg-zinc-700"
            >
              <RefreshCw className="h-4 w-4" />
              Try Again
            </button>
          </div>
        </div>
      )
    }

    return this.props.children
  }
}
