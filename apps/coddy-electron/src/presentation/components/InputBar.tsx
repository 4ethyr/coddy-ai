// presentation/components/InputBar.tsx
// Terminal-style textarea input: Enter sends, Shift+Enter newlines, auto-resize.

import {
  useState,
  useRef,
  useCallback,
  useMemo,
  useEffect,
  useLayoutEffect,
  type KeyboardEvent,
  type ChangeEvent,
} from 'react'
import { Icon } from './Icon'
import {
  listUiSlashCommandSuggestions,
  type UiSlashCommandSuggestion,
} from '@/presentation/commands/slashCommands'

interface Props {
  onSend: (text: string) => void
  disabled?: boolean
  placeholder?: string
}

const MAX_ROWS = 6

export function InputBar({
  onSend,
  disabled = false,
  placeholder = 'Enter command or prompt...',
}: Props) {
  const [value, setValue] = useState('')
  const [dismissedSuggestionsFor, setDismissedSuggestionsFor] = useState<
    string | null
  >(null)
  const [activeSuggestionIndex, setActiveSuggestionIndex] = useState(0)
  const textareaRef = useRef<HTMLTextAreaElement>(null)
  const pendingCaretPositionRef = useRef<number | null>(null)
  const slashSuggestions = useMemo(() => {
    if (disabled || dismissedSuggestionsFor === value) return []
    return listUiSlashCommandSuggestions(value)
  }, [disabled, dismissedSuggestionsFor, value])

  useEffect(() => {
    setActiveSuggestionIndex((current) =>
      slashSuggestions.length > 0
        ? Math.min(current, slashSuggestions.length - 1)
        : 0,
    )
  }, [slashSuggestions.length])

  useLayoutEffect(() => {
    const caret = pendingCaretPositionRef.current
    if (caret === null) return

    pendingCaretPositionRef.current = null

    const textarea = textareaRef.current
    if (!textarea) return

    textarea.focus()
    textarea.setSelectionRange(caret, caret)
    resizeTextarea(textarea)
  }, [value])

  const submit = useCallback(() => {
    const text = value.trim()
    if (!text || disabled) return
    onSend(text)
    setValue('')
    setDismissedSuggestionsFor(null)
    // Reset textarea height
    if (textareaRef.current) {
      textareaRef.current.style.height = 'auto'
    }
  }, [value, disabled, onSend])

  const handleChange = useCallback(
    (e: ChangeEvent<HTMLTextAreaElement>) => {
      setValue(e.target.value)
      resizeTextarea(e.target)
    },
    [],
  )

  const pasteClipboardText = useCallback(
    async (textarea: HTMLTextAreaElement) => {
      const text = await navigator.clipboard?.readText?.()
      if (!text) return

      const start = textarea.selectionStart ?? value.length
      const end = textarea.selectionEnd ?? start
      const next = `${value.slice(0, start)}${text}${value.slice(end)}`
      const caret = start + text.length
      setValue(next)

      requestAnimationFrame(() => {
        textarea.setSelectionRange(caret, caret)
        resizeTextarea(textarea)
      })
    },
    [value],
  )

  const selectSlashSuggestion = useCallback(
    (suggestion: UiSlashCommandSuggestion) => {
      const next = suggestion.insertText
      pendingCaretPositionRef.current = next.length
      setValue(next)
      setDismissedSuggestionsFor(next)
    },
    [],
  )

  const handleKeyDown = useCallback(
    (e: KeyboardEvent<HTMLTextAreaElement>) => {
      if (isPasteShortcut(e)) {
        e.preventDefault()
        void pasteClipboardText(e.currentTarget)
        return
      }

      if (slashSuggestions.length > 0) {
        if (e.key === 'Escape') {
          e.preventDefault()
          setDismissedSuggestionsFor(value)
          return
        }

        if (e.key === 'ArrowDown') {
          e.preventDefault()
          setActiveSuggestionIndex((current) =>
            (current + 1) % slashSuggestions.length,
          )
          return
        }

        if (e.key === 'ArrowUp') {
          e.preventDefault()
          setActiveSuggestionIndex((current) =>
            (current - 1 + slashSuggestions.length) % slashSuggestions.length,
          )
          return
        }

        if ((e.key === 'Enter' && !e.shiftKey) || e.key === 'Tab') {
          e.preventDefault()
          const activeSuggestion = slashSuggestions[activeSuggestionIndex]
          if (activeSuggestion) {
            selectSlashSuggestion(activeSuggestion)
          }
          return
        }
      }

      if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault()
        submit()
      }
    },
    [
      activeSuggestionIndex,
      pasteClipboardText,
      selectSlashSuggestion,
      slashSuggestions,
      submit,
      value,
    ],
  )

  return (
    <div className="terminal-input relative flex items-start rounded-full border border-outline-variant/80 bg-surface-container/70 px-1 shadow-[inset_0_1px_0_rgba(255,255,255,0.04)]">
      {slashSuggestions.length > 0 && (
        <div
          role="listbox"
          aria-label="Slash commands"
          className="absolute bottom-full left-2 right-2 z-40 mb-2 overflow-hidden rounded-lg border border-primary/20 bg-surface-container-high/95 p-1 shadow-2xl backdrop-blur-xl"
        >
          {slashSuggestions.map((suggestion, index) => (
            <button
              key={suggestion.command}
              type="button"
              role="option"
              aria-selected={index === activeSuggestionIndex}
              aria-label={`${suggestion.command} ${suggestion.title}`}
              onMouseDown={(event) => {
                event.preventDefault()
              }}
              onClick={() => selectSlashSuggestion(suggestion)}
              className={`flex w-full min-w-0 flex-col rounded-md px-3 py-2 text-left transition-colors ${
                index === activeSuggestionIndex
                  ? 'bg-primary/15 text-on-surface'
                  : 'text-on-surface-variant hover:bg-white/5 hover:text-on-surface'
              }`}
            >
              <span className="flex min-w-0 items-center gap-2 font-mono text-xs">
                <span className="text-primary">{suggestion.command}</span>
                <span className="truncate">{suggestion.title}</span>
              </span>
              <span className="mt-0.5 truncate text-[11px] text-on-surface-muted">
                {suggestion.description}
              </span>
            </button>
          ))}
        </div>
      )}
      <span className="select-none pl-4 pt-3 font-mono text-sm leading-5 text-primary drop-shadow-[0_0_8px_rgba(0,219,233,0.65)]">
        &gt;
      </span>

      <textarea
        ref={textareaRef}
        className="min-h-[42px] flex-1 resize-none overflow-y-auto border-none bg-transparent px-3 py-2.5 font-mono text-sm leading-5 text-on-surface caret-primary outline-none placeholder:text-on-surface-variant/45 focus:ring-0"
        placeholder={placeholder}
        rows={1}
        value={value}
        onChange={handleChange}
        onKeyDown={handleKeyDown}
        disabled={disabled}
      />

      <button
        type="button"
        onClick={submit}
        disabled={disabled || !value.trim()}
        className="mr-2 mt-1.5 flex h-8 w-8 flex-shrink-0 items-center justify-center rounded-full bg-primary/10 text-primary shadow-[0_0_18px_rgba(0,219,233,0.12)] transition-colors hover:bg-primary/20 disabled:cursor-not-allowed disabled:opacity-30"
        title="Send"
        aria-label="Send"
      >
        <Icon name="send" className="h-4 w-4" />
      </button>
    </div>
  )
}

function isPasteShortcut(event: KeyboardEvent<HTMLTextAreaElement>): boolean {
  return (
    (event.ctrlKey || event.metaKey)
    && event.shiftKey
    && event.key.toLowerCase() === 'v'
  )
}

function resizeTextarea(textarea: HTMLTextAreaElement): void {
  textarea.style.height = 'auto'
  const lineHeight = 20
  const maxHeight = lineHeight * MAX_ROWS
  textarea.style.height = `${Math.min(textarea.scrollHeight, maxHeight)}px`
}
