import { useEffect, useRef, useState } from 'react'
import {
  MODEL_PROVIDER_CATALOG,
  getModelCatalogEntry,
  getModelProvider,
  type ModelCatalogEntry,
  type ModelProviderOption,
  type ModelRef,
} from '@/domain'
import { Icon } from './Icon'

interface Props {
  model: ModelRef
  onSelect?: (model: ModelRef) => void
}

export function ModelSelector({ model, onSelect }: Props) {
  const [isOpen, setIsOpen] = useState(false)
  const rootRef = useRef<HTMLDivElement>(null)
  const activeProvider = getModelProvider(model.provider)
  const activeModel = getModelCatalogEntry(model)
  const activeLabel = `${model.provider}/${model.name}`
  const activeProviderLabel = activeProvider?.shortLabel ?? model.provider

  useEffect(() => {
    if (!isOpen) return

    const handlePointerDown = (event: PointerEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) {
        setIsOpen(false)
      }
    }

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setIsOpen(false)
    }

    document.addEventListener('pointerdown', handlePointerDown)
    document.addEventListener('keydown', handleKeyDown)

    return () => {
      document.removeEventListener('pointerdown', handlePointerDown)
      document.removeEventListener('keydown', handleKeyDown)
    }
  }, [isOpen])

  return (
    <div ref={rootRef} className="relative">
      <button
        type="button"
        aria-expanded={isOpen}
        aria-haspopup="menu"
        onClick={() => setIsOpen((value) => !value)}
        className="flex items-center gap-2 rounded-full border border-outline-variant/80 bg-surface-container-high/80 px-3 py-1 font-mono text-[11px] uppercase tracking-[0.08em] text-on-surface-variant transition-colors hover:border-primary/50 hover:text-primary"
        aria-label={`Active model ${activeLabel}`}
      >
        <span className="h-1.5 w-1.5 rounded-full bg-primary shadow-[0_0_8px_rgba(0,219,233,0.9)]" />
        <span>MODEL: {activeModel?.label ?? model.name}</span>
        <span className="hidden text-primary/70 sm:inline">{activeProviderLabel}</span>
        <Icon
          name="chevronDown"
          className={`h-3.5 w-3.5 transition-transform ${isOpen ? 'rotate-180' : ''}`}
        />
      </button>

      <div
        className={`absolute right-0 top-full z-[140] pt-2 transition duration-150 ease-out ${
          isOpen
            ? 'translate-y-0 opacity-100'
            : 'pointer-events-none -translate-y-1 opacity-0'
        }`}
      >
        <div
          className="flex max-h-[min(620px,calc(100vh-96px))] min-w-[360px] max-w-[calc(100vw-32px)] flex-col gap-2 overflow-y-auto rounded-lg border border-outline-variant/70 bg-surface-container-high/95 p-2 shadow-[0_20px_40px_rgba(0,0,0,0.5)] backdrop-blur-[30px]"
          aria-label="Model provider catalog"
        >
          {MODEL_PROVIDER_CATALOG.map((provider) => (
            <ProviderGroup
              key={provider.id}
              provider={provider}
              activeModel={model}
              onSelect={(next) => {
                onSelect?.(next)
                setIsOpen(false)
              }}
            />
          ))}
        </div>
      </div>
    </div>
  )
}

function ProviderGroup({
  provider,
  activeModel,
  onSelect,
}: {
  provider: ModelProviderOption
  activeModel: ModelRef
  onSelect: (model: ModelRef) => void
}) {
  return (
    <section className="rounded-md border border-white/5 bg-surface-container-low/45 p-2">
      <div className="mb-2 flex items-start justify-between gap-3 px-1">
        <div className="min-w-0">
          <h3 className="font-display text-xs font-medium uppercase tracking-[0.16em] text-on-surface">
            {provider.label}
          </h3>
          <p className="mt-1 line-clamp-2 text-xs leading-5 text-on-surface-variant/70">
            {provider.description}
          </p>
        </div>
        <span className="rounded border border-primary/20 bg-primary/10 px-2 py-1 font-mono text-[9px] uppercase tracking-[0.14em] text-primary/80">
          {provider.connectionLabel}
        </span>
      </div>

      <div className="flex flex-col gap-1">
        {provider.models.map((entry) => (
          <ModelOptionButton
            key={`${entry.model.provider}/${entry.model.name}`}
            provider={provider}
            entry={entry}
            active={
              entry.model.provider === activeModel.provider &&
              entry.model.name === activeModel.name
            }
            onSelect={onSelect}
          />
        ))}
      </div>
    </section>
  )
}

function ModelOptionButton({
  provider,
  entry,
  active,
  onSelect,
}: {
  provider: ModelProviderOption
  entry: ModelCatalogEntry
  active: boolean
  onSelect: (model: ModelRef) => void
}) {
  return (
    <button
      type="button"
      aria-pressed={active}
      aria-label={`Select ${entry.label} via ${provider.label}`}
      className={`flex w-full items-center justify-between gap-3 rounded-md border-l-2 px-3 py-2 text-left transition-colors ${
        active
          ? 'border-primary bg-primary/10 text-primary shadow-[inset_16px_0_32px_-16px_rgba(0,240,255,0.35)]'
          : 'border-transparent text-on-surface-variant hover:bg-surface-bright/40 hover:text-on-surface'
      }`}
      onClick={() => onSelect(entry.model)}
    >
      <span className="flex min-w-0 items-center gap-3">
        <span className="h-1.5 w-1.5 flex-shrink-0 rounded-full bg-primary/80 shadow-[0_0_8px_rgba(0,219,233,0.65)]" />
        <span className="min-w-0">
          <span className="block truncate font-mono text-sm">{entry.label}</span>
          <span className="mt-0.5 block truncate text-[11px] text-on-surface-variant/60">
            {entry.description}
          </span>
        </span>
      </span>
      <span className="flex flex-shrink-0 items-center gap-2">
        <span className="hidden font-mono text-[9px] uppercase tracking-[0.14em] text-on-surface-variant/50 sm:inline">
          {provider.routingLabel}
        </span>
        <Icon
          name={provider.connectionKind === 'local' ? 'cpu' : 'cloud'}
          className="h-4 w-4 opacity-60"
        />
      </span>
    </button>
  )
}
