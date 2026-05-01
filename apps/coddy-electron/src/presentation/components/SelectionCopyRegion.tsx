import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type HTMLAttributes,
  type MouseEvent,
} from 'react'

type CopyMenuState = {
  text: string
  x: number
  y: number
}

type SelectionCopyRegionProps = HTMLAttributes<HTMLDivElement>

export function SelectionCopyRegion({
  children,
  className,
  ...props
}: SelectionCopyRegionProps) {
  const regionRef = useRef<HTMLDivElement>(null)
  const [menu, setMenu] = useState<CopyMenuState | null>(null)

  const copySelection = useCallback(async (text: string) => {
    await copyTextToClipboard(text)
    setMenu(null)
  }, [])

  const handleContextMenu = useCallback(
    (event: MouseEvent<HTMLDivElement>) => {
      const text = getSelectedText()
      if (!text) return

      event.preventDefault()
      setMenu({
        text,
        x: event.clientX,
        y: event.clientY,
      })
    },
    [],
  )

  useEffect(() => {
    const handlePointerDown = (event: PointerEvent) => {
      if (!menu) return
      if (regionRef.current?.contains(event.target as Node)) return
      setMenu(null)
    }

    const handleKeyDown = (event: KeyboardEvent) => {
      if (!isCopySelectionShortcut(event)) return
      if (isEditableElement(document.activeElement)) return

      const text = getSelectedText()
      if (!text) return

      event.preventDefault()
      void copySelection(text)
    }

    document.addEventListener('pointerdown', handlePointerDown)
    window.addEventListener('keydown', handleKeyDown)
    return () => {
      document.removeEventListener('pointerdown', handlePointerDown)
      window.removeEventListener('keydown', handleKeyDown)
    }
  }, [copySelection, menu])

  return (
    <div
      ref={regionRef}
      className={className}
      onContextMenu={handleContextMenu}
      {...props}
    >
      {children}
      {menu && (
        <button
          type="button"
          className="fixed z-[260] rounded-md border border-primary/35 bg-surface-container-high px-3 py-2 font-mono text-xs text-primary shadow-[0_18px_42px_rgba(0,0,0,0.55)] transition-colors hover:bg-primary/10"
          style={{ left: menu.x, top: menu.y }}
          onClick={() => {
            void copySelection(menu.text)
          }}
        >
          Copy selection
        </button>
      )}
    </div>
  )
}

function isCopySelectionShortcut(event: KeyboardEvent): boolean {
  return (
    (event.ctrlKey || event.metaKey)
    && event.shiftKey
    && event.key.toLowerCase() === 'c'
  )
}

function getSelectedText(): string {
  return window.getSelection?.()?.toString().trim() ?? ''
}

function isEditableElement(element: Element | null): boolean {
  if (!element) return false
  if (element instanceof HTMLInputElement) return true
  if (element instanceof HTMLTextAreaElement) return true
  if (element instanceof HTMLSelectElement) return true
  return element instanceof HTMLElement && element.isContentEditable
}

async function copyTextToClipboard(text: string): Promise<void> {
  if (navigator.clipboard?.writeText) {
    await navigator.clipboard.writeText(text)
  }
}
