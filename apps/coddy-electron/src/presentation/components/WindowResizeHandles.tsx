import { useCallback } from 'react'
import type { PointerEvent as ReactPointerEvent } from 'react'

export type ResizeEdge = 'n' | 's' | 'e' | 'w' | 'ne' | 'nw' | 'se' | 'sw'

const HANDLES: Array<{ edge: ResizeEdge; className: string }> = [
  { edge: 'n', className: 'left-3 right-3 top-0 h-2 cursor-n-resize' },
  { edge: 's', className: 'bottom-0 left-3 right-3 h-2 cursor-s-resize' },
  { edge: 'e', className: 'bottom-3 right-0 top-3 w-2 cursor-e-resize' },
  { edge: 'w', className: 'bottom-3 left-0 top-3 w-2 cursor-w-resize' },
  { edge: 'ne', className: 'right-0 top-0 h-4 w-4 cursor-ne-resize' },
  { edge: 'nw', className: 'left-0 top-0 h-4 w-4 cursor-nw-resize' },
  { edge: 'se', className: 'bottom-0 right-0 h-4 w-4 cursor-se-resize' },
  { edge: 'sw', className: 'bottom-0 left-0 h-4 w-4 cursor-sw-resize' },
]

export function WindowResizeHandles() {
  const handlePointerDown = useCallback(
    (edge: ResizeEdge, event: ReactPointerEvent<HTMLDivElement>) => {
      if (event.button > 0) return
      if (typeof window === 'undefined' || !window.replApi) return

      event.preventDefault()
      event.stopPropagation()
      try {
        event.currentTarget.setPointerCapture(event.pointerId)
      } catch {
        // Pointer capture is best-effort; the main process also polls cursor position.
      }
      const startPoint = pointerPoint(event)

      const payload = {
        edge,
        screenX: startPoint.screenX,
        screenY: startPoint.screenY,
      }

      document.body.classList.add('window-resizing')
      void window.replApi.invoke('window:resize-start', payload)

      const handlePointerMove = (moveEvent: PointerEvent) => {
        const point = pointerPoint(moveEvent)
        void window.replApi.invoke('window:resize-drag', {
          screenX: point.screenX,
          screenY: point.screenY,
        })
      }

      const handlePointerUp = () => {
        document.body.classList.remove('window-resizing')
        window.removeEventListener('pointermove', handlePointerMove)
        window.removeEventListener('pointerup', handlePointerUp)
        window.removeEventListener('pointercancel', handlePointerUp)
        window.removeEventListener('blur', handlePointerUp)
        void window.replApi.invoke('window:resize-end')
      }

      window.addEventListener('pointermove', handlePointerMove)
      window.addEventListener('pointerup', handlePointerUp)
      window.addEventListener('pointercancel', handlePointerUp)
      window.addEventListener('blur', handlePointerUp)
    },
    [],
  )

  return (
    <div className="pointer-events-none fixed inset-0 z-[115]" aria-hidden="true">
      {HANDLES.map((handle) => (
        <div
          key={handle.edge}
          data-testid={`window-resize-${handle.edge}`}
          className={`pointer-events-auto absolute ${handle.className}`}
          onPointerDown={(event) => handlePointerDown(handle.edge, event)}
        />
      ))}
    </div>
  )
}

function pointerPoint(event: Pick<PointerEvent, 'screenX' | 'screenY' | 'clientX' | 'clientY'>) {
  return {
    screenX: event.screenX ?? event.clientX,
    screenY: event.screenY ?? event.clientY,
  }
}
