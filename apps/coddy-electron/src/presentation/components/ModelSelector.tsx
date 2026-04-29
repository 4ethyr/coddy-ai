import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type FormEvent,
  type WheelEvent as ReactWheelEvent,
} from 'react'
import {
  MODEL_PROVIDER_CATALOG,
  getModelCatalogEntry,
  getModelProvider,
  type ModelCatalogEntry,
  type ModelProviderId,
  type ModelProviderListRequest,
  type ModelProviderListResult,
  type ModelProviderOption,
  type ModelRef,
} from '@/domain'
import { Icon } from './Icon'

interface Props {
  model: ModelRef
  onSelect?: (model: ModelRef) => void
  onLoadModels?: (
    request: ModelProviderListRequest,
  ) => Promise<ModelProviderListResult>
}

type ProviderModelMap = Partial<Record<ModelProviderId, ModelCatalogEntry[]>>
type ProviderLoadStatus = 'idle' | 'loading' | 'ready' | 'error'
type ProviderLoadMap = Partial<
  Record<
    ModelProviderId,
    {
      status: ProviderLoadStatus
      message?: string
      fetchedAtUnixMs?: number
    }
  >
>
type ProviderCredentialDraftMap = Partial<
  Record<
    ModelProviderId,
    {
      apiKey: string
      endpoint: string
      rememberCredential: boolean
    }
  >
>
type CredentialDraftField = 'apiKey' | 'endpoint' | 'rememberCredential'
type CredentialDraftValue = string | boolean
type MenuFrame = {
  top: number
  left: number
  width: number
  maxHeight: number
}

const DEFAULT_MENU_FRAME: MenuFrame = {
  top: 64,
  left: 16,
  width: 420,
  maxHeight: 520,
}

export function ModelSelector({ model, onSelect, onLoadModels }: Props) {
  const [isOpen, setIsOpen] = useState(false)
  const [searchQuery, setSearchQuery] = useState('')
  const [menuFrame, setMenuFrame] = useState<MenuFrame>(DEFAULT_MENU_FRAME)
  const [providerModels, setProviderModels] = useState<ProviderModelMap>(() =>
    getInitialProviderModels(),
  )
  const [providerLoadState, setProviderLoadState] =
    useState<ProviderLoadMap>({})
  const [credentialDrafts, setCredentialDrafts] =
    useState<ProviderCredentialDraftMap>({})
  const rootRef = useRef<HTMLDivElement>(null)
  const triggerRef = useRef<HTMLButtonElement>(null)
  const activeProvider = getModelProvider(model.provider)
  const activeModel =
    findModelEntry(model, providerModels) ?? getModelCatalogEntry(model)
  const activeLabel = `${model.provider}/${model.name}`
  const activeProviderLabel = activeProvider?.shortLabel ?? model.provider

  const updateMenuFrame = useCallback(() => {
    setMenuFrame(calculateMenuFrame(triggerRef.current?.getBoundingClientRect()))
  }, [])

  const handleMenuWheel = (event: ReactWheelEvent<HTMLDivElement>) => {
    const menu = event.currentTarget
    const maxScrollTop = menu.scrollHeight - menu.clientHeight

    event.stopPropagation()
    if (maxScrollTop <= 0) return

    event.preventDefault()
    const nextScrollTop = menu.scrollTop + getWheelDeltaPixels(event, menu)
    menu.scrollTop = Math.max(0, Math.min(maxScrollTop, nextScrollTop))
  }

  const loadProviderModels = async (provider: ModelProviderOption) => {
    if (!onLoadModels) {
      setProviderLoadState((current) => ({
        ...current,
        [provider.id]: {
          status: 'error',
          message: 'Model provider bridge unavailable.',
        },
      }))
      return
    }

    setProviderLoadState((current) => ({
      ...current,
      [provider.id]: { status: 'loading' },
    }))

    const draft = getCredentialDraft(credentialDrafts, provider.id)
    let result: ModelProviderListResult
    try {
      result = await onLoadModels({
        provider: provider.id,
        apiKey: draft.apiKey.trim() || undefined,
        endpoint: draft.endpoint.trim() || undefined,
        ...(draft.rememberCredential ? { rememberCredential: true } : {}),
      })
    } catch {
      clearCredentialDraft(provider.id)
      setProviderLoadState((current) => ({
        ...current,
        [provider.id]: {
          status: 'error',
          message: 'Unable to load provider models.',
        },
      }))
      return
    }

    clearCredentialDraft(provider.id)

    if (result.error) {
      setProviderLoadState((current) => ({
        ...current,
        [provider.id]: {
          status: 'error',
          message: result.error?.message ?? 'Unable to load provider models.',
        },
      }))
      return
    }

    setProviderModels((current) => ({
      ...current,
      [provider.id]: result.models,
    }))
    setProviderLoadState((current) => ({
      ...current,
      [provider.id]: {
        status: 'ready',
        message: getLoadedStatusMessage(result),
        fetchedAtUnixMs: result.fetchedAtUnixMs,
      },
    }))
  }

  const clearCredentialDraft = (provider: ModelProviderId) => {
    setCredentialDrafts((current) => ({
      ...current,
      [provider]: {
        ...getCredentialDraft(current, provider),
        apiKey: '',
      },
    }))
  }

  useEffect(() => {
    if (!isOpen || !onLoadModels) return

    const ollama = MODEL_PROVIDER_CATALOG.find(
      (provider) => provider.id === 'ollama',
    )
    if (!ollama) return

    const state = providerLoadState.ollama?.status ?? 'idle'
    if (state === 'idle') {
      void loadProviderModels(ollama)
    }
  }, [isOpen, onLoadModels, providerLoadState.ollama?.status])

  useEffect(() => {
    if (!isOpen) return

    updateMenuFrame()

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
    window.addEventListener('resize', updateMenuFrame)
    window.addEventListener('scroll', updateMenuFrame, true)

    return () => {
      document.removeEventListener('pointerdown', handlePointerDown)
      document.removeEventListener('keydown', handleKeyDown)
      window.removeEventListener('resize', updateMenuFrame)
      window.removeEventListener('scroll', updateMenuFrame, true)
    }
  }, [isOpen, updateMenuFrame])

  const filteredProviders = getFilteredProviderGroups(
    providerModels,
    searchQuery,
  )

  return (
    <div ref={rootRef} className="relative">
      <button
        ref={triggerRef}
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
        data-testid="model-selector-popover"
        className={`model-selector-popover fixed z-[220] transition duration-150 ease-out ${
          isOpen
            ? 'translate-y-0 opacity-100'
            : 'pointer-events-none -translate-y-1 opacity-0'
        }`}
        style={{
          left: `${menuFrame.left}px`,
          top: `${menuFrame.top}px`,
          width: `${menuFrame.width}px`,
          maxWidth: 'calc(100vw - 24px)',
        }}
      >
        <div
          data-testid="model-selector-menu"
          className="model-selector-menu flex w-full min-w-0 flex-col gap-2 rounded-lg border border-outline-variant/80 p-2 pr-3 shadow-[0_24px_56px_rgba(0,0,0,0.72)]"
          aria-label="Model provider catalog"
          onWheelCapture={handleMenuWheel}
          style={{ maxHeight: `${menuFrame.maxHeight}px` }}
        >
          <div className="model-selector-search sticky top-0 z-10 rounded-md border border-white/[0.08] bg-surface-container-high/95 p-2 backdrop-blur-xl">
            <label className="flex items-center gap-2 rounded border border-outline-variant/60 bg-surface-dim/70 px-3 py-2 text-on-surface-variant focus-within:border-primary/50">
              <Icon name="search" className="h-4 w-4 text-primary/80" />
              <span className="sr-only">Search models or providers</span>
              <input
                value={searchQuery}
                onChange={(event) => setSearchQuery(event.target.value)}
                placeholder="Search provider or model..."
                className="min-w-0 flex-1 bg-transparent font-mono text-xs text-on-surface outline-none placeholder:text-on-surface-variant/45"
              />
            </label>
          </div>

          {filteredProviders.length === 0 ? (
            <div className="rounded-md border border-white/[0.08] bg-surface-container-low/80 px-4 py-5 text-center font-mono text-xs text-on-surface-variant/60">
              No provider or model matches this search.
            </div>
          ) : (
            filteredProviders.map(({ provider, models }) => (
              <ProviderGroup
                key={provider.id}
                provider={provider}
                models={models}
                activeModel={model}
                loadState={providerLoadState[provider.id]}
                draft={getCredentialDraft(credentialDrafts, provider.id)}
                onDraftChange={(field, value) => {
                  setCredentialDrafts((current) => ({
                    ...current,
                    [provider.id]: {
                      ...getCredentialDraft(current, provider.id),
                      [field]: value,
                    },
                  }))
                }}
                onLoad={() => {
                  void loadProviderModels(provider)
                }}
                onSelect={(next) => {
                  onSelect?.(next)
                  setIsOpen(false)
                }}
              />
            ))
          )}
        </div>
      </div>
    </div>
  )
}

function ProviderGroup({
  provider,
  models,
  activeModel,
  loadState,
  draft,
  onDraftChange,
  onLoad,
  onSelect,
}: {
  provider: ModelProviderOption
  models: readonly ModelCatalogEntry[]
  activeModel: ModelRef
  loadState?: ProviderLoadMap[ModelProviderId]
  draft: { apiKey: string; endpoint: string; rememberCredential: boolean }
  onDraftChange: (field: CredentialDraftField, value: CredentialDraftValue) => void
  onLoad: () => void
  onSelect: (model: ModelRef) => void
}) {
  const status = loadState?.status ?? 'idle'
  const hasModels = models.length > 0

  const handleSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    onLoad()
  }

  return (
    <section
      data-testid="model-provider-group"
      data-provider={provider.id}
      className="shrink-0 rounded-md border border-white/[0.08] bg-surface-container-low/90 p-2 shadow-[inset_0_1px_0_rgba(255,255,255,0.03)]"
    >
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

      <form
        className="mb-2 flex flex-col gap-2 rounded-md border border-white/[0.06] bg-surface-dim/45 p-2"
        onSubmit={handleSubmit}
      >
        {provider.requiresCredential && (
          <label className="flex min-w-0 items-center gap-2 rounded border border-outline-variant/50 bg-surface-container/60 px-2 py-1.5 focus-within:border-primary/50">
            <Icon name="lock" className="h-3.5 w-3.5 flex-shrink-0 text-primary/70" />
            <span className="sr-only">
              {provider.credentialLabel ?? 'Provider credential'}
            </span>
            <input
              type="password"
              autoComplete="off"
              value={draft.apiKey}
              placeholder={provider.credentialPlaceholder ?? 'API key'}
              onChange={(event) => onDraftChange('apiKey', event.target.value)}
              className="min-w-0 flex-1 bg-transparent font-mono text-[11px] text-on-surface outline-none placeholder:text-on-surface-variant/45"
            />
          </label>
        )}

        {provider.requiresEndpoint && (
          <label className="flex min-w-0 items-center gap-2 rounded border border-outline-variant/50 bg-surface-container/60 px-2 py-1.5 focus-within:border-primary/50">
            <Icon name="cloud" className="h-3.5 w-3.5 flex-shrink-0 text-primary/70" />
            <span className="sr-only">
              {provider.endpointLabel ?? 'Provider endpoint'}
            </span>
            <input
              type="url"
              value={draft.endpoint}
              placeholder={provider.endpointPlaceholder ?? 'https://...'}
              onChange={(event) => onDraftChange('endpoint', event.target.value)}
              className="min-w-0 flex-1 bg-transparent font-mono text-[11px] text-on-surface outline-none placeholder:text-on-surface-variant/45"
            />
          </label>
        )}

        {provider.requiresCredential && (
          <label className="flex items-center gap-2 px-1 font-mono text-[10px] uppercase tracking-[0.12em] text-on-surface-variant/60">
            <input
              type="checkbox"
              checked={draft.rememberCredential}
              onChange={(event) =>
                onDraftChange('rememberCredential', event.target.checked)
              }
              className="h-3 w-3 accent-primary"
            />
            <span>Remember securely</span>
          </label>
        )}

        <div className="flex items-center justify-between gap-3">
          <span
            className={`min-w-0 truncate font-mono text-[10px] uppercase tracking-[0.12em] ${
              status === 'error'
                ? 'text-red-300'
                : status === 'ready'
                  ? 'text-primary/80'
                  : 'text-on-surface-variant/50'
            }`}
          >
            {getStatusLabel(provider, loadState, hasModels)}
          </span>
          <button
            type="submit"
            disabled={status === 'loading'}
            className="flex-shrink-0 rounded border border-primary/25 bg-primary/10 px-2 py-1 font-mono text-[9px] uppercase tracking-[0.14em] text-primary/80 transition-colors hover:bg-primary/15 disabled:cursor-wait disabled:opacity-60"
          >
            {status === 'loading' ? 'Loading' : hasModels ? 'Refresh' : 'Load'}
          </button>
        </div>
      </form>

      {hasModels ? (
        <div className="flex flex-col gap-1">
          {models.map((entry) => (
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
      ) : (
        <div className="rounded-md border border-dashed border-outline-variant/40 px-3 py-4 text-center font-mono text-[11px] leading-5 text-on-surface-variant/55">
          {provider.requiresCredential
            ? 'Connect this provider to list available models.'
            : 'Load local runtime models.'}
        </div>
      )}
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
          ? 'border-primary bg-primary/15 text-primary shadow-[inset_16px_0_32px_-16px_rgba(0,240,255,0.35)]'
          : 'border-transparent text-on-surface-variant hover:bg-surface-bright/70 hover:text-on-surface'
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

function getInitialProviderModels(): ProviderModelMap {
  return MODEL_PROVIDER_CATALOG.reduce<ProviderModelMap>((result, provider) => {
    if (provider.models.length > 0) {
      result[provider.id] = [...provider.models]
    }
    return result
  }, {})
}

function calculateMenuFrame(rect: DOMRect | undefined): MenuFrame {
  const viewportWidth = window.innerWidth || 800
  const viewportHeight = window.innerHeight || 600
  const margin = 12
  const availableWidth = Math.max(0, viewportWidth - margin * 2)
  const width = Math.min(560, availableWidth)
  const anchorRect = getUsableAnchorRect(rect)
  const anchorRight = anchorRect?.right ?? viewportWidth - margin
  const maxLeft = viewportWidth - width - margin
  const left = clampNumber(anchorRight - width, margin, Math.max(margin, maxLeft))
  const idealTop = (anchorRect?.bottom ?? 56) + 8
  const top = clampNumber(idealTop, margin, Math.max(margin, viewportHeight - 280))
  const availableHeight = Math.max(160, viewportHeight - top - margin)
  const maxHeight = Math.min(620, availableHeight)

  return {
    top: Math.round(top),
    left: Math.round(left),
    width: Math.round(width),
    maxHeight: Math.round(maxHeight),
  }
}

function getUsableAnchorRect(rect: DOMRect | undefined): DOMRect | undefined {
  if (!rect) return undefined
  const values = [rect.top, rect.right, rect.bottom, rect.left]
  const hasFiniteValues = values.every(Number.isFinite)
  const hasVisibleBox = rect.width > 0 || rect.height > 0
  return hasFiniteValues && hasVisibleBox ? rect : undefined
}

function getFilteredProviderGroups(
  providerModels: ProviderModelMap,
  query: string,
): Array<{ provider: ModelProviderOption; models: ModelCatalogEntry[] }> {
  const normalizedQuery = query.trim().toLowerCase()

  return MODEL_PROVIDER_CATALOG.map((provider) => {
    const models = providerModels[provider.id] ?? []
    const filteredModels = normalizedQuery
      ? models.filter((entry) => modelMatches(entry, normalizedQuery))
      : models
    return {
      provider,
      models: filteredModels,
      providerMatches: providerMatches(provider, normalizedQuery),
    }
  })
    .filter(
      ({ models, providerMatches }) =>
        !normalizedQuery || providerMatches || models.length > 0,
    )
    .map(({ provider, models }) => ({ provider, models }))
}

function providerMatches(
  provider: ModelProviderOption,
  normalizedQuery: string,
): boolean {
  if (!normalizedQuery) return true
  return [
    provider.id,
    provider.label,
    provider.shortLabel,
    provider.description,
    provider.routingLabel,
    provider.connectionLabel,
  ].some((value) => value.toLowerCase().includes(normalizedQuery))
}

function modelMatches(
  entry: ModelCatalogEntry,
  normalizedQuery: string,
): boolean {
  return [
    entry.model.provider,
    entry.model.name,
    entry.label,
    entry.description,
    ...entry.tags,
  ].some((value) => value.toLowerCase().includes(normalizedQuery))
}

function findModelEntry(
  model: ModelRef,
  providerModels: ProviderModelMap,
): ModelCatalogEntry | undefined {
  return Object.values(providerModels)
    .flatMap((entries) => entries ?? [])
    .find(
      (entry) =>
        entry.model.provider === model.provider &&
        entry.model.name === model.name,
    )
}

function getCredentialDraft(
  drafts: ProviderCredentialDraftMap,
  provider: ModelProviderId,
): { apiKey: string; endpoint: string; rememberCredential: boolean } {
  return (
    drafts[provider] ?? {
      apiKey: '',
      endpoint: '',
      rememberCredential: false,
    }
  )
}

function getLoadedStatusMessage(result: ModelProviderListResult): string {
  if (result.credentialStorage) {
    return result.credentialStorage.persisted
      ? `${result.models.length} models loaded - saved securely`
      : `${result.models.length} models loaded - credential not saved`
  }
  return `${result.models.length} models loaded`
}

function getStatusLabel(
  provider: ModelProviderOption,
  state: ProviderLoadMap[ModelProviderId] | undefined,
  hasModels: boolean,
): string {
  if (state?.status === 'loading') return 'loading provider models'
  if (state?.status === 'error') return state.message ?? 'provider unavailable'
  if (state?.status === 'ready') return state.message ?? 'models loaded'
  if (hasModels) return `${provider.models.length} cached models`
  return provider.requiresCredential ? 'credential required' : 'local discovery'
}

function getWheelDeltaPixels(
  event: ReactWheelEvent<HTMLDivElement>,
  menu: HTMLDivElement,
) {
  if (event.deltaMode === WheelEvent.DOM_DELTA_LINE) {
    return event.deltaY * 16
  }

  if (event.deltaMode === WheelEvent.DOM_DELTA_PAGE) {
    return event.deltaY * menu.clientHeight
  }

  return event.deltaY
}

function clampNumber(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value))
}
