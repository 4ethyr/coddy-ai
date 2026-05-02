// presentation/components/MessageBubble.tsx
// Renders one REPL transcript entry, not a chat bubble.

import type { ReplMessage } from '@/domain'
import type { JSX, ReactNode } from 'react'
import { CodeBlock, parseMarkdown } from './CodeBlock'
import { Icon } from './Icon'

interface Props {
  message: ReplMessage
}

export function MessageBubble({ message }: Props) {
  const isUser = message.role === 'user'

  if (isUser) {
    return (
      <div className="group flex w-full items-start gap-3 font-mono text-sm">
        <span className="mt-0.5 text-primary drop-shadow-[0_0_8px_rgba(0,219,233,0.65)]">
          &gt;
        </span>
        <div className="min-w-0 flex-1">
          <div className="mb-1 flex items-center gap-2 text-[10px] uppercase tracking-[0.22em] text-on-surface-variant/45">
            <Icon
              name="user"
              className="h-3.5 w-3.5"
              data-testid="user-message-icon"
            />
            user_input
          </div>
          <p className="break-words text-on-surface/95">{message.text}</p>
        </div>
      </div>
    )
  }

  return (
    <div className="flex w-full items-start gap-4">
      <div className="mt-1 flex h-8 w-8 flex-shrink-0 items-center justify-center rounded-md border border-primary bg-primary/10 text-primary shadow-[0_0_22px_rgba(0,219,233,0.2)]">
        <Icon
          name="bot"
          className="h-4 w-4"
          data-testid="assistant-message-icon"
        />
      </div>

      <div className="coddy-message-content min-w-0 flex-1 rounded-lg border border-outline-variant/70 bg-surface-container/45 px-5 py-4 shadow-[inset_0_1px_0_rgba(255,255,255,0.03)]">
        <div className="mb-2 font-mono text-[10px] uppercase tracking-[0.22em] text-primary/80">
          coddy_agent
        </div>
        <MarkdownContent text={message.text} />
      </div>
    </div>
  )
}

export function MarkdownContent({ text }: { text: string }): JSX.Element {
  const segments = parseMarkdown(text)

  return (
    <div className="coddy-markdown flex flex-col gap-3 text-sm leading-6 text-on-surface">
      {segments.map((seg, i) =>
        seg.type === 'code' ? (
          <CodeBlock key={i} code={seg.content} language={seg.language} />
        ) : (
          <MarkdownText key={i} text={seg.content} />
        ),
      )}
    </div>
  )
}

function MarkdownText({ text }: { text: string }): JSX.Element {
  const blocks = parseMarkdownBlocks(text)

  return (
    <>
      {blocks.map((block, index) => {
        if (block.type === 'heading') {
          const headingClass =
            block.level <= 2
              ? 'text-lg font-semibold text-on-surface'
              : 'text-base font-semibold text-on-surface'
          return (
            <div
              key={index}
              className={`${headingClass} coddy-markdown-heading mt-2 first:mt-0`}
            >
              {renderInlineMarkdown(block.content)}
            </div>
          )
        }

        if (block.type === 'ordered-list') {
          return (
            <ol
              key={index}
              className="ml-5 list-decimal space-y-1 break-words"
            >
              {block.items.map((item, itemIndex) => (
                <li key={itemIndex}>{renderInlineMarkdown(item)}</li>
              ))}
            </ol>
          )
        }

        if (block.type === 'unordered-list') {
          return (
            <ul key={index} className="ml-5 list-disc space-y-1 break-words">
              {block.items.map((item, itemIndex) => (
                <li key={itemIndex}>{renderInlineMarkdown(item)}</li>
              ))}
            </ul>
          )
        }

        if (block.type === 'quote') {
          return (
            <blockquote
              key={index}
              className="border-l-2 border-primary/50 pl-4 text-on-surface-variant"
            >
              {renderInlineMarkdown(block.content)}
            </blockquote>
          )
        }

        if (block.type === 'table') {
          return (
            <div key={index} className="overflow-x-auto rounded border border-white/10">
              <table className="min-w-full border-collapse text-left text-sm">
                <thead className="bg-surface-container-high/60 text-on-surface">
                  <tr>
                    {block.headers.map((header, headerIndex) => (
                      <th
                        key={headerIndex}
                        scope="col"
                        className="border-b border-white/10 px-3 py-2 font-semibold"
                      >
                        {renderInlineMarkdown(header)}
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {block.rows.map((row, rowIndex) => (
                    <tr key={rowIndex} className="border-t border-white/5">
                      {block.headers.map((_, cellIndex) => (
                        <td key={cellIndex} className="px-3 py-2 align-top">
                          {renderInlineMarkdown(row[cellIndex] ?? '')}
                        </td>
                      ))}
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )
        }

        return (
          <p key={index} className="whitespace-pre-wrap break-words">
            {renderInlineMarkdown(block.content)}
          </p>
        )
      })}
    </>
  )
}

type MarkdownBlock =
  | { type: 'paragraph'; content: string }
  | { type: 'heading'; level: number; content: string }
  | { type: 'ordered-list'; items: string[] }
  | { type: 'unordered-list'; items: string[] }
  | { type: 'quote'; content: string }
  | { type: 'table'; headers: string[]; rows: string[][] }

function parseMarkdownBlocks(text: string): MarkdownBlock[] {
  const blocks: MarkdownBlock[] = []
  const lines = text.replace(/\r\n/g, '\n').split('\n')
  let index = 0

  while (index < lines.length) {
    const line = lines[index] ?? ''
    const trimmed = line.trim()
    if (!trimmed) {
      index += 1
      continue
    }

    const heading = trimmed.match(/^(#{1,6})\s+(.+)$/)
    if (heading) {
      blocks.push({
        type: 'heading',
        level: heading[1]!.length,
        content: heading[2]!.trim(),
      })
      index += 1
      continue
    }

    if (isMarkdownTableStart(lines, index)) {
      const headers = parseMarkdownTableRow(trimmed)
      index += 2
      const rows: string[][] = []
      while (index < lines.length) {
        const rowLine = (lines[index] ?? '').trim()
        if (!rowLine || !looksLikeMarkdownTableRow(rowLine)) break
        rows.push(parseMarkdownTableRow(rowLine))
        index += 1
      }
      blocks.push({ type: 'table', headers, rows })
      continue
    }

    const orderedItem = trimmed.match(/^\d+[.)]\s+(.+)$/)
    if (orderedItem) {
      const items: string[] = []
      while (index < lines.length) {
        const match = (lines[index] ?? '').trim().match(/^\d+[.)]\s+(.+)$/)
        if (!match) break
        items.push(match[1]!.trim())
        index += 1
      }
      blocks.push({ type: 'ordered-list', items })
      continue
    }

    const unorderedItem = trimmed.match(/^[-*+]\s+(.+)$/)
    if (unorderedItem) {
      const items: string[] = []
      while (index < lines.length) {
        const match = (lines[index] ?? '').trim().match(/^[-*+]\s+(.+)$/)
        if (!match) break
        items.push(match[1]!.trim())
        index += 1
      }
      blocks.push({ type: 'unordered-list', items })
      continue
    }

    const quote = trimmed.match(/^>\s?(.+)$/)
    if (quote) {
      const quoteLines: string[] = []
      while (index < lines.length) {
        const match = (lines[index] ?? '').trim().match(/^>\s?(.+)$/)
        if (!match) break
        quoteLines.push(match[1]!.trim())
        index += 1
      }
      blocks.push({ type: 'quote', content: quoteLines.join(' ') })
      continue
    }

    const paragraphLines: string[] = []
    while (index < lines.length) {
      const candidate = lines[index] ?? ''
      const candidateTrimmed = candidate.trim()
      if (!candidateTrimmed) break
      if (
        /^(#{1,6})\s+/.test(candidateTrimmed)
        || /^\d+[.)]\s+/.test(candidateTrimmed)
        || /^[-*+]\s+/.test(candidateTrimmed)
        || /^>\s?/.test(candidateTrimmed)
        || isMarkdownTableStart(lines, index)
      ) {
        break
      }
      paragraphLines.push(candidateTrimmed)
      index += 1
    }
    blocks.push({ type: 'paragraph', content: paragraphLines.join(' ') })
  }

  return blocks
}

function isMarkdownTableStart(lines: string[], index: number): boolean {
  const header = (lines[index] ?? '').trim()
  const separator = (lines[index + 1] ?? '').trim()
  if (!looksLikeMarkdownTableRow(header)) return false
  const cells = parseMarkdownTableRow(header)
  if (cells.length < 2) return false
  const separatorCells = parseMarkdownTableRow(separator)
  if (separatorCells.length < cells.length) return false
  return separatorCells.every((cell) =>
    /^:?-{3,}:?$/.test(cell),
  )
}

function looksLikeMarkdownTableRow(line: string): boolean {
  return line.includes('|') && parseMarkdownTableRow(line).length >= 2
}

function parseMarkdownTableRow(line: string): string[] {
  const trimmed = line.trim().replace(/^\|/, '').replace(/\|$/, '')
  return trimmed.split('|').map((cell) => cell.trim())
}

function renderInlineMarkdown(text: string): ReactNode[] {
  const nodes: ReactNode[] = []
  const pattern =
    /(\*\*[^*]+?\*\*|__[^_]+?__|`[^`]+?`|\[[^\]]+\]\([^)]+\)|\*[^*\n]+?\*|_[^_\n]+?_)/g
  let lastIndex = 0
  let match: RegExpExecArray | null

  while ((match = pattern.exec(text)) !== null) {
    if (match.index > lastIndex) {
      nodes.push(text.slice(lastIndex, match.index))
    }
    nodes.push(renderInlineToken(match[0], nodes.length))
    lastIndex = pattern.lastIndex
  }

  if (lastIndex < text.length) {
    nodes.push(text.slice(lastIndex))
  }

  return nodes
}

function renderInlineToken(token: string, key: number): ReactNode {
  if (token.startsWith('**') && token.endsWith('**')) {
    return (
      <strong key={key} className="coddy-markdown-strong font-semibold">
        {renderInlineMarkdown(token.slice(2, -2))}
      </strong>
    )
  }

  if (token.startsWith('__') && token.endsWith('__')) {
    return (
      <strong key={key} className="coddy-markdown-strong font-semibold">
        {renderInlineMarkdown(token.slice(2, -2))}
      </strong>
    )
  }

  if (token.startsWith('*') && token.endsWith('*')) {
    return (
      <em key={key} className="coddy-markdown-emphasis italic">
        {renderInlineMarkdown(token.slice(1, -1))}
      </em>
    )
  }

  if (token.startsWith('_') && token.endsWith('_')) {
    return (
      <em key={key} className="coddy-markdown-emphasis italic">
        {renderInlineMarkdown(token.slice(1, -1))}
      </em>
    )
  }

  if (token.startsWith('`') && token.endsWith('`')) {
    return (
      <code
        key={key}
        className="rounded border border-primary/20 bg-surface-container-high/70 px-1.5 py-0.5 font-mono text-[0.92em] text-primary"
      >
        {token.slice(1, -1)}
      </code>
    )
  }

  const link = token.match(/^\[([^\]]+)\]\(([^)]+)\)$/)
  if (link) {
    const href = sanitizeHref(link[2]!)
    return href ? (
      <a
        key={key}
        href={href}
        target="_blank"
        rel="noreferrer"
        className="text-primary underline decoration-primary/40 underline-offset-4"
      >
        {renderInlineMarkdown(link[1]!)}
      </a>
    ) : (
      link[1]
    )
  }

  return token
}

function sanitizeHref(href: string): string | null {
  const value = href.trim()
  return /^(https?:|mailto:)/i.test(value) ? value : null
}
